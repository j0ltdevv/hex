use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::app_settings::AppSettings;

const TEST_KEY: &str = "test-key-not-secret";
const PROMPT: &str = "names: Camille; terms: ORCA";
const TRANSCRIPT: &str = "Bonjour, \"Groq\" !\nça va";
const SAMPLES: &[f32] = &[
    0.0,
    0.25,
    -0.25,
    1.0,
    -1.0,
    2.0,
    -2.0,
    f32::NAN,
    f32::INFINITY,
    f32::NEG_INFINITY,
];

#[test]
fn debug_output_never_contains_the_api_key() {
    let key = GroqApiKey::from(TEST_KEY);
    assert_eq!(format!("{key:?}"), "[redacted]");
    assert!(!format!("{key:?}").contains(TEST_KEY));
}

#[test]
fn key_status_reports_only_the_source_and_never_the_key() {
    let status = key_status();
    let shown = format!("{status:?}");
    assert!(
        ["None", "Environment", "KeyFile", "Settings", "Blocked"].contains(&shown.as_str()),
        "unexpected status {shown:?}"
    );
    assert!(!shown.contains(TEST_KEY));
}

#[test]
fn provider_defaults_to_local_and_serde_metadata_stays_backward_compatible() {
    assert_eq!(SpeechProvider::default(), SpeechProvider::Local);

    assert_eq!(
        serde_json::to_string(&SpeechProvider::Local).unwrap(),
        "\"local\""
    );
    assert_eq!(
        serde_json::to_string(&SpeechProvider::Groq).unwrap(),
        "\"groq\""
    );
    assert_eq!(
        serde_json::from_str::<SpeechProvider>("\"local\"").unwrap(),
        SpeechProvider::Local
    );
    assert_eq!(
        serde_json::from_str::<SpeechProvider>("\"groq\"").unwrap(),
        SpeechProvider::Groq
    );
    assert!(serde_json::from_str::<SpeechProvider>("\"Groq\"").is_err());

    // A settings file written before the field existed, and explicit Groq.
    let old: AppSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.speech_provider, SpeechProvider::Local);
    let chosen: AppSettings = serde_json::from_str(r#"{"speech_provider":"groq"}"#).unwrap();
    assert_eq!(chosen.speech_provider, SpeechProvider::Groq);
    assert_eq!(
        serde_json::to_value(&old).unwrap()["speech_provider"],
        "local"
    );
}

#[test]
fn a_saved_key_does_not_turn_the_local_default_into_a_request() {
    let mut settings = AppSettings::default();
    settings.speech_provider = SpeechProvider::Local;
    settings.groq_api_key = Some(GroqApiKey::from(TEST_KEY));

    assert_eq!(provider_from(&settings), SpeechProvider::Local);
    assert!(resolve_key(None, None, Some(TEST_KEY), true).is_some());

    // The saved key remains available for an explicit Groq selection; the
    // provider, not the key, is the gate used by the engine and worker.
    settings.speech_provider = SpeechProvider::Groq;
    assert_eq!(provider_from(&settings), SpeechProvider::Groq);
}

#[test]
fn selecting_groq_without_any_key_stays_on_the_local_path() {
    let mut settings = AppSettings::default();
    settings.speech_provider = SpeechProvider::Groq;
    settings.groq_api_key = None;

    assert_eq!(provider_from(&settings), SpeechProvider::Groq);
    assert_eq!(resolve_key(None, None, None, true), None);

    settings.groq_api_key = Some(GroqApiKey::from(TEST_KEY));
    assert_eq!(
        resolve_key(None, None, Some(TEST_KEY), true)
            .unwrap()
            .as_str(),
        TEST_KEY
    );
}

#[test]
fn key_precedence_falls_from_environment_to_key_file_to_saved_setting() {
    assert_eq!(
        resolve_key(
            Some("  env-key \n"),
            Some("file-key"),
            Some("saved-key"),
            true
        )
        .unwrap()
        .as_str(),
        "env-key"
    );
    assert_eq!(
        resolve_key(None, Some("  file-key  "), Some("saved-key"), true)
            .unwrap()
            .as_str(),
        "file-key"
    );
    assert_eq!(
        resolve_key(None, None, Some(" saved-key \n"), true)
            .unwrap()
            .as_str(),
        "saved-key"
    );
    assert_eq!(
        resolve_key(Some(""), Some(" file-key "), Some("saved-key"), true)
            .unwrap()
            .as_str(),
        "file-key"
    );
    assert_eq!(resolve_key(None, None, None, true), None);
    assert_eq!(
        resolve_key(Some(" \t"), Some("\n"), Some(" \n"), true),
        None
    );
}

#[test]
fn an_isolated_data_root_ignores_system_keys_but_keeps_its_own_key() {
    // allow_system=false models HEX_APPLICATION_SUPPORT_DIR plus
    // HEX_GROQ_ALLOW_SYSTEM_KEYS=0: no environment/system key is borrowed.
    assert_eq!(
        resolve_key(
            Some(" env-key "),
            Some(" file-key "),
            Some("saved-key"),
            false
        )
        .unwrap()
        .as_str(),
        "file-key"
    );
    assert_eq!(
        resolve_key(Some(" env-key "), None, Some(" saved-key "), false)
            .unwrap()
            .as_str(),
        "saved-key"
    );
    assert_eq!(resolve_key(Some(" env-key "), None, None, false), None);
}

#[test]
fn key_file_is_owner_only_and_never_yields_a_world_readable_key() {
    let dir = TempDir::new("key-file");
    let path = key_file_path(dir.path());
    assert_eq!(path, dir.path().join("groq.key"));

    write_key_file(&path, &GroqApiKey::from(TEST_KEY)).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), TEST_KEY);
    assert_eq!(read_key_file(&path).unwrap(), GroqApiKey::from(TEST_KEY));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the key file must be owner-only");
        let parent_mode = fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(parent_mode, 0o700, "write must not widen the data root");

        fs::write(&path, TEST_KEY).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(read_key_file(&path), None);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(read_key_file(&path), None);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join("groq.key.link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(
            read_key_file(&link),
            None,
            "a symlink must not yield the key it points to"
        );
    }

    assert_eq!(read_key_file(&dir.path().join("missing.key")), None);
}

#[test]
fn groq_model_default_and_wire_names_are_stable() {
    assert_eq!(GroqModel::default(), GroqModel::LargeV3);
    assert_eq!(GroqModel::LargeV3.as_str(), "whisper-large-v3");
    assert_eq!(GroqModel::Turbo.as_str(), "whisper-large-v3-turbo");
    assert_eq!(
        serde_json::to_string(&GroqModel::LargeV3).unwrap(),
        "\"whisper-large-v3\""
    );
    assert_eq!(
        serde_json::to_string(&GroqModel::Turbo).unwrap(),
        "\"whisper-large-v3-turbo\""
    );
    assert_eq!(
        serde_json::from_str::<GroqModel>("\"whisper-large-v3\"").unwrap(),
        GroqModel::LargeV3
    );
    assert_eq!(
        serde_json::from_str::<GroqModel>("\"whisper-large-v3-turbo\"").unwrap(),
        GroqModel::Turbo
    );
    assert!(serde_json::from_str::<GroqModel>("\"large-v3\"").is_err());
}

#[test]
fn an_owned_request_keeps_samples_language_prompt_and_model_off_the_worker() {
    let request = GroqRequest::new(SAMPLES, Some("fr"), Some(PROMPT), GroqModel::Turbo);
    assert!(
        request.samples.len() == SAMPLES.len()
            && request
                .samples
                .iter()
                .zip(SAMPLES)
                .all(|(left, right)| left.to_bits() == right.to_bits())
    );
    assert_eq!(request.language, Some("fr"));
    assert_eq!(request.prompt, Some(PROMPT));
    assert_eq!(request.model, GroqModel::Turbo);

    let owned = request.into_owned();
    assert!(
        owned.samples.len() == SAMPLES.len()
            && owned
                .samples
                .iter()
                .zip(SAMPLES)
                .all(|(left, right)| left.to_bits() == right.to_bits())
    );
    assert_eq!(owned.language.as_deref(), Some("fr"));
    assert_eq!(owned.prompt.as_deref(), Some(PROMPT));
    assert_eq!(owned.model, GroqModel::Turbo);
}

#[test]
fn wav_has_a_mono_16_khz_16_bit_riff_header_and_clamped_pcm() {
    let bytes = wav(SAMPLES);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
        bytes.len() - 8
    );
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(&bytes[12..16], b"fmt ");
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 16);
    assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        16_000
    );
    assert_eq!(
        u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
        32_000
    );
    assert_eq!(u16::from_le_bytes(bytes[32..34].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16);
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
        SAMPLES.len() * 2
    );
    assert_eq!(bytes.len(), 44 + SAMPLES.len() * 2);

    let pcm: Vec<i16> = bytes[44..]
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    assert_eq!(pcm, [0, 8191, -8192, 32767, -32768, 32767, -32768, 0, 0, 0]);
}

#[test]
fn empty_audio_still_has_a_complete_wav_header() {
    let bytes = wav(&[]);
    assert_eq!(bytes.len(), 44);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36);
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 0);
}

#[test]
fn language_and_prompt_are_sent_as_multipart_fields() {
    let (endpoint, server) = mock_server(&[("200 OK", success_json())]);
    let agent = crate::net::loopback_agent(|config| config);
    let key = GroqApiKey::from(TEST_KEY);
    let request = GroqRequest::new(SAMPLES, Some("fr"), Some(PROMPT), GroqModel::LargeV3);

    let text =
        transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new()).unwrap();

    assert_eq!(text, TRANSCRIPT);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_upload(
        &requests[0],
        GroqModel::LargeV3,
        Some("fr"),
        Some(PROMPT),
        "dictation.flac",
        "audio/flac",
        &flac::encode(SAMPLES),
    );
}

#[test]
fn absent_language_and_prompt_are_not_sent_as_empty_fields() {
    let (endpoint, server) = mock_server(&[("200 OK", success_json())]);
    let agent = crate::net::loopback_agent(|config| config);
    let key = GroqApiKey::from(TEST_KEY);
    let request = GroqRequest::new(SAMPLES, None, None, GroqModel::Turbo);

    let text =
        transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new()).unwrap();

    assert_eq!(text, TRANSCRIPT);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_upload(
        &requests[0],
        GroqModel::Turbo,
        None,
        None,
        "dictation.flac",
        "audio/flac",
        &flac::encode(SAMPLES),
    );
}

#[test]
fn a_415_or_422_retries_once_with_wav_and_keeps_all_fields() {
    let flac = flac::encode(SAMPLES);
    let wav = wav(SAMPLES);
    assert_ne!(flac, wav, "the retry must change the encoded audio");

    for status in ["415 Unsupported Media Type", "422 Unprocessable Entity"] {
        let (endpoint, server) = mock_server(&[
            (status, r#"{"error":{"message":"unsupported audio"}}"#),
            ("200 OK", success_json()),
        ]);
        let agent = crate::net::loopback_agent(|config| config);
        let key = GroqApiKey::from(TEST_KEY);
        let request = GroqRequest::new(SAMPLES, Some("fr"), Some(PROMPT), GroqModel::Turbo);

        let text =
            transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new()).unwrap();

        assert_eq!(text, TRANSCRIPT, "{status}");
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2, "{status}");
        assert_upload(
            &requests[0],
            GroqModel::Turbo,
            Some("fr"),
            Some(PROMPT),
            "dictation.flac",
            "audio/flac",
            &flac,
        );
        assert_upload(
            &requests[1],
            GroqModel::Turbo,
            Some("fr"),
            Some(PROMPT),
            "dictation.wav",
            "audio/wav",
            &wav,
        );
    }
}

#[test]
fn other_http_failures_and_a_failed_wav_retry_are_not_retried_again() {
    let key = GroqApiKey::from(TEST_KEY);
    let agent = crate::net::loopback_agent(|config| config);

    for status in [
        "400 Bad Request",
        "401 Unauthorized",
        "429 Too Many Requests",
        "500 Internal Server Error",
    ] {
        let (endpoint, server) = mock_server(&[(status, r#"{"error":{"message":"no"}}"#)]);
        let request = GroqRequest::new(SAMPLES, None, None, GroqModel::LargeV3);
        let error = transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new())
            .unwrap_err();

        assert!(!error.trim().is_empty(), "{status}");
        assert!(!error.contains(TEST_KEY), "{status}");
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1, "{status}: no WAV retry");
    }

    let (endpoint, server) = mock_server(&[
        (
            "415 Unsupported Media Type",
            r#"{"error":{"message":"no flac"}}"#,
        ),
        (
            "422 Unprocessable Entity",
            r#"{"error":{"message":"no wav"}}"#,
        ),
    ]);
    let request = GroqRequest::new(SAMPLES, None, None, GroqModel::LargeV3);
    let error =
        transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new()).unwrap_err();

    assert!(!error.trim().is_empty());
    assert!(!error.contains(TEST_KEY));
    assert_eq!(server.join().unwrap().len(), 2, "only FLAC then WAV");
}

#[test]
fn malformed_or_empty_transcripts_are_errors_without_a_retry() {
    let key = GroqApiKey::from(TEST_KEY);
    let agent = crate::net::loopback_agent(|config| config);

    for response in ["not json", "{}", r#"{"text":""}"#, r#"{"text":"  "}"#] {
        let (endpoint, server) = mock_server(&[("200 OK", response)]);
        let request = GroqRequest::new(SAMPLES, None, None, GroqModel::LargeV3);
        let error = transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new())
            .unwrap_err();

        assert!(!error.trim().is_empty(), "response was {response}");
        assert!(!error.contains(TEST_KEY));
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[test]
fn the_wav_only_transcribe_seam_still_sends_a_wav_upload() {
    let (endpoint, server) = mock_server(&[("200 OK", success_json())]);
    let agent = crate::net::loopback_agent(|config| config);

    let text = transcribe_with(&agent, &endpoint, TEST_KEY, GroqModel::LargeV3, SAMPLES).unwrap();

    assert_eq!(text, TRANSCRIPT);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_upload(
        &requests[0],
        GroqModel::LargeV3,
        None,
        None,
        "dictation.wav",
        "audio/wav",
        &wav(SAMPLES),
    );
}

#[test]
fn cancellation_before_a_request_skips_the_request_entirely() {
    let cancel = Cancellation::new();
    assert!(!cancel.is_cancelled());
    cancel.cancel();
    assert!(cancel.is_cancelled());

    let (endpoint, server) = mock_server(&[("200 OK", success_json())]);
    let agent = crate::net::loopback_agent(|config| config);
    let key = GroqApiKey::from(TEST_KEY);
    let request = GroqRequest::new(SAMPLES, None, None, GroqModel::LargeV3);
    let error = transcribe_with_retry(&agent, &endpoint, &key, request, &cancel).unwrap_err();

    assert!(!error.trim().is_empty());
    assert!(!error.contains(TEST_KEY));
    assert!(
        server.join().unwrap().is_empty(),
        "a cancelled job sends nothing"
    );
}

#[test]
fn cancellation_during_a_failed_attempt_prevents_the_wav_retry() {
    let cancel = Cancellation::new();
    let (endpoint, server) = mock_server_cancelling(
        &[(
            "415 Unsupported Media Type",
            r#"{"error":{"message":"no flac"}}"#,
        )],
        cancel.clone(),
    );
    let agent = crate::net::loopback_agent(|config| config);
    let key = GroqApiKey::from(TEST_KEY);
    let request = GroqRequest::new(SAMPLES, None, None, GroqModel::LargeV3);
    let error = transcribe_with_retry(&agent, &endpoint, &key, request, &cancel).unwrap_err();

    assert!(!error.trim().is_empty());
    assert!(!error.contains(TEST_KEY));
    assert!(cancel.is_cancelled(), "clones share the cancellation flag");
    assert_eq!(server.join().unwrap().len(), 1, "no retry after cancel");
}

fn pending_with(
    reply: Option<Result<String, String>>,
    cancel: CancelToken,
    closing_mark_required: bool,
) -> PendingGroq {
    let (sender, receiver) = mpsc::channel();
    match reply {
        Some(reply) => sender.send(reply).unwrap(),
        None => drop(sender),
    }
    PendingGroq {
        reply: receiver,
        cancel,
        closing_mark_required,
    }
}

#[test]
fn settle_accepts_cloud_text_and_reports_the_cloud_provider() {
    let pending = pending_with(Some(Ok("cloud".into())), CancelToken::new(), false);
    let settled = pending.settle("local", |text| Ok(format!("{text}!")));
    assert_eq!(
        settled,
        Settled {
            value: "cloud!".into(),
            cloud_used: true,
        }
    );
}

#[test]
fn settle_keeps_local_text_on_cloud_error_rejection_and_disconnect() {
    for reply in [
        Some(Err("cloud failed".into())),
        Some(Ok("rejected".into())),
        None,
    ] {
        let pending = pending_with(reply, CancelToken::new(), false);
        let settled = pending.settle("local", |text| {
            (text != "rejected")
                .then_some(text)
                .ok_or_else(|| "bad cloud text".into())
        });
        assert_eq!(
            settled,
            Settled {
                value: "local".into(),
                cloud_used: false,
            }
        );
    }
}

#[test]
fn settle_refuses_cloud_text_queued_before_cancellation() {
    let cancel = CancelToken::new();
    let pending = pending_with(Some(Ok("cloud".into())), cancel.clone(), false);
    cancel.cancel();
    let settled = pending.settle("local", |text| Ok(text));
    assert_eq!(settled.value, "local");
    assert!(!settled.cloud_used);
}

#[test]
fn a_failed_local_run_still_settles_an_already_started_cloud_job() {
    let pending = pending_with(Some(Ok("cloud".into())), CancelToken::new(), false);
    let settled = settle_local(Some(pending), Err("local failed".to_owned()), |text| {
        Ok(text)
    });
    assert_eq!(
        settled,
        Settled {
            value: Ok("cloud".into()),
            cloud_used: true,
        }
    );

    // A rejected cloud result keeps the original local error, and the
    // closing-mark rule captured with the job still applies.
    let pending = pending_with(Some(Ok("cloud".into())), CancelToken::new(), true);
    let settled = settle_local(Some(pending), Err("local failed".to_owned()), |text| {
        crate::transcription::engine::accept_cloud_text(text, true)
    });
    assert_eq!(settled.value, Err("local failed".to_owned()));
    assert!(!settled.cloud_used);

    // A failed cloud result is cancelled before the local error propagates.
    let cancel = CancelToken::new();
    let pending = pending_with(Some(Err("cloud failed".into())), cancel.clone(), false);
    let settled = settle_local(Some(pending), Err("local failed".to_owned()), |text| {
        Ok(text)
    });
    assert_eq!(settled.value, Err("local failed".to_owned()));
    assert!(cancel.is_cancelled());

    let cancel = CancelToken::new();
    cancel.cancel();
    let pending = pending_with(Some(Ok("cloud".into())), cancel, false);
    let settled = settle_local(Some(pending), Err("local failed".to_owned()), |text| {
        Ok(text)
    });
    assert_eq!(settled.value, Err("local failed".to_owned()));
    assert!(!settled.cloud_used);

    let settled = settle_local::<&str>(None, Err("local failed"), |text| Ok(text));
    assert_eq!(settled.value, Err("local failed"));
    assert!(!settled.cloud_used);
}

#[test]
fn dropping_a_rejected_pending_job_cancels_the_worker() {
    let cancel = CancelToken::new();
    let pending = pending_with(None, cancel.clone(), false);
    drop(pending);
    assert!(cancel.is_cancelled());
}

#[test]
fn the_key_is_snapshotted_when_the_cloud_job_starts() {
    let request = || OwnedGroqRequest::new(SAMPLES.to_vec(), None, None, GroqModel::LargeV3);
    let cancel = CancelToken::new();
    assert!(PendingGroq::start_with_key(Some(request()), None, cancel.clone(), false).is_none());
    assert!(!cancel.is_cancelled(), "no key means no cloud job at all");
    // `spawn` uses the snapshot, not a later lookup: an empty snapshot
    // fails configuration before any request can be sent.
    let receiver = spawn(request(), GroqApiKey::default(), CancelToken::new());
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(2)),
        Ok(Err(error)) if error.contains("key is empty")
    ));

    // Pre-cancelled so the spawned worker cannot reach the network.
    let cancel = CancelToken::new();
    cancel.cancel();
    let pending = PendingGroq::start_with_key(
        Some(request()),
        Some(GroqApiKey::from(TEST_KEY)),
        cancel,
        true,
    )
    .unwrap();
    assert!(pending.closing_mark_required());
    assert_eq!(pending.settle("local", |text| Ok(text)).value, "local");
}

#[test]
fn closing_mark_rule_comes_from_the_captured_model_selection() {
    let selection = crate::transcription::models::TranscriptionSelection::default();
    assert!(closing_mark_required(Some(&selection)));
    assert!(!closing_mark_required(None));
}

#[test]
fn boundaries_are_fresh_for_each_request() {
    let boundary = boundary();
    assert!(boundary.starts_with("hex-groq-"));
    assert_eq!(boundary.len(), "hex-groq-".len() + 32);
    assert!(
        boundary["hex-groq-".len()..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
}

#[test]
fn hostile_prompt_and_language_cannot_inject_multipart_fields() {
    let prompt = "Camille\r\n--hex-groq-dictation\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\ninjected";
    let language = "fr\r\n--hex-groq-dictation\r\nContent-Disposition: form-data; name=\"file\"";
    let (endpoint, server) = mock_server(&[("200 OK", success_json())]);
    let agent = crate::net::loopback_agent(|config| config);
    let key = GroqApiKey::from(TEST_KEY);
    let request = GroqRequest::new(SAMPLES, Some(language), Some(prompt), GroqModel::Turbo);

    let text =
        transcribe_with_retry(&agent, &endpoint, &key, request, &Cancellation::new()).unwrap();
    assert_eq!(text, TRANSCRIPT);

    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    let content_type = requests[0].header("content-type").unwrap();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap();
    assert!(
        !boundary.contains("hex-groq-dictation"),
        "the old fixed boundary must not be reused"
    );
    assert_upload(
        &requests[0],
        GroqModel::Turbo,
        Some(language),
        Some(prompt),
        "dictation.flac",
        "audio/flac",
        &flac::encode(SAMPLES),
    );
}

#[test]
fn usage_records_are_bounded_and_contain_no_key_or_user_text() {
    let record = usage::UsageRecord {
        at_unix_secs: 1_700_000_000,
        provider: SpeechProvider::Groq,
        model: GroqModel::Turbo,
        outcome: usage::UsageOutcome::WavRetry,
        samples: 16_000,
        duration_ms: 420,
    };
    let debug = format!("{record:?}");
    for expected in ["Groq", "Turbo", "WavRetry", "16000", "420"] {
        assert!(debug.contains(expected), "{debug:?} lost {expected:?}");
    }
    for secret in [TEST_KEY, PROMPT, "fr", "Bonjour"] {
        assert!(!debug.contains(secret), "usage record leaked {secret:?}");
    }
    let lower = debug.to_ascii_lowercase();
    for field in ["key", "prompt", "audio", "transcript", "language"] {
        assert!(!lower.contains(field), "usage record exposed {field:?}");
    }

    const BASE: u64 = 1_800_000_000;
    for index in 0..70 {
        usage::record(usage::UsageRecord {
            at_unix_secs: BASE + index,
            provider: SpeechProvider::Groq,
            model: GroqModel::LargeV3,
            outcome: if index % 2 == 0 {
                usage::UsageOutcome::Succeeded
            } else {
                usage::UsageOutcome::Failed
            },
            samples: index as usize,
            duration_ms: index,
        });
    }
    let recent: Vec<_> = usage::snapshot()
        .into_iter()
        .filter(|entry| (BASE..BASE + 70).contains(&entry.at_unix_secs))
        .collect();
    assert_eq!(recent.len(), 64, "the ring must be bounded at 64 entries");
    assert_eq!(recent[0].at_unix_secs, BASE + 69);
    assert!(
        recent
            .windows(2)
            .all(|pair| pair[0].at_unix_secs > pair[1].at_unix_secs)
    );
}

// Opt-in live smoke: set HEX_GROQ_LIVE_FIXTURE to a 16 kHz mono WAV and
// GROQ_API_KEY to a real key, then run this ignored test. Neither is printed.
#[test]
#[ignore = "requires HEX_GROQ_LIVE_FIXTURE and GROQ_API_KEY for a live request"]
fn live_groq_fixture_transcribes_nonempty_text() {
    let Some(fixture) = std::env::var_os("HEX_GROQ_LIVE_FIXTURE").filter(|path| !path.is_empty())
    else {
        return;
    };
    let Some(key) = std::env::var("GROQ_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
    else {
        return;
    };
    let fixture = std::path::PathBuf::from(fixture);
    let key = GroqApiKey::from(key.trim());
    let mut reader = hound::WavReader::open(&fixture).expect("could not read live WAV fixture");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1, "fixture must be mono");
    assert_eq!(spec.sample_rate, 16_000, "fixture must be 16 kHz");
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .expect("fixture contains invalid float samples"),
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|sample| sample as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .expect("fixture contains invalid PCM samples")
        }
    };
    assert!(!samples.is_empty(), "fixture must contain audio");

    let agent = crate::net::agent(ENDPOINT, |config| config);
    let request = GroqRequest::new(&samples, None, None, GroqModel::LargeV3);
    let text = transcribe_with_retry(&agent, ENDPOINT, &key, request, &Cancellation::new())
        .expect("live Groq transcription failed");

    assert!(!text.trim().is_empty());
}

fn success_json() -> &'static str {
    r#"{"text":"Bonjour, \"Groq\" !\n\u00e7a va","language":"fr"}"#
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("hex-groq-{tag}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct CapturedRequest {
    request_line: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

fn mock_server(
    responses: &[(&'static str, &'static str)],
) -> (String, thread::JoinHandle<Vec<CapturedRequest>>) {
    let owned: Vec<(String, String)> = responses
        .iter()
        .map(|(status, response)| (status.to_string(), response.to_string()))
        .collect();
    mock_server_impl(owned, None)
}

fn mock_server_cancelling(
    responses: &[(&'static str, &'static str)],
    cancel: Cancellation,
) -> (String, thread::JoinHandle<Vec<CapturedRequest>>) {
    let owned: Vec<(String, String)> = responses
        .iter()
        .map(|(status, response)| (status.to_string(), response.to_string()))
        .collect();
    mock_server_impl(owned, Some(cancel))
}

/// A loopback fixture that records every request it sees. It stops after a
/// short quiet period, so an unexpected extra (or missing) retry is visible
/// through the returned vector instead of hanging the test.
fn mock_server_impl(
    responses: Vec<(String, String)>,
    cancel_after_first: Option<Cancellation>,
) -> (String, thread::JoinHandle<Vec<CapturedRequest>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        let started = Instant::now();
        let mut last = started;
        loop {
            let expired = if requests.is_empty() {
                started.elapsed() >= Duration::from_secs(1)
            } else {
                last.elapsed() >= Duration::from_millis(400)
            };
            if expired {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let (status, response) = &responses[requests.len().min(responses.len() - 1)];
                    requests.push(read_request(&mut stream, status, response));
                    if requests.len() == 1 {
                        if let Some(cancel) = &cancel_after_first {
                            cancel.cancel();
                        }
                    }
                    last = Instant::now();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("loopback accept failed: {error}"),
            }
        }
        requests
    });
    (
        format!("http://{address}/openai/v1/audio/transcriptions"),
        server,
    )
}

fn read_request(stream: &mut TcpStream, status: &str, response: &str) -> CapturedRequest {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();

    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
    }
    let length = headers
        .get("content-length")
        .expect("multipart request must declare its length")
        .parse::<usize>()
        .unwrap();
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();

    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    )
    .unwrap();
    stream.flush().unwrap();

    CapturedRequest {
        request_line: request_line.trim_end().to_owned(),
        headers,
        body,
    }
}

struct MultipartPart<'a> {
    headers: BTreeMap<String, String>,
    body: &'a [u8],
}

impl MultipartPart<'_> {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn disposition_parameter(&self, parameter: &str) -> Option<&str> {
        self.header("content-disposition")?
            .split(';')
            .skip(1)
            .find_map(|item| {
                let (name, value) = item.trim().split_once('=')?;
                (name == parameter).then(|| value.trim_matches('"'))
            })
    }
}

fn multipart_parts<'a>(body: &'a [u8], boundary: &str) -> Vec<MultipartPart<'a>> {
    let marker = format!("--{boundary}").into_bytes();
    let mut parts = Vec::new();
    let mut search = body;
    while let Some(start) = search
        .windows(marker.len())
        .position(|window| window == marker.as_slice())
    {
        let after = &search[start + marker.len()..];
        let end = after
            .windows(marker.len())
            .position(|window| window == marker.as_slice())
            .unwrap_or(after.len());
        let section = &after[..end];
        search = &after[end..];
        if search.len() < marker.len() {
            break;
        }
        let Some(section) = section.strip_prefix(b"\r\n") else {
            continue;
        };
        let Some(section) = section.strip_suffix(b"\r\n") else {
            continue;
        };
        if section == b"--" {
            continue;
        }
        let separator = section
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap();
        let header_text = std::str::from_utf8(&section[..separator]).unwrap();
        let headers = header_text
            .lines()
            .map(|line| {
                let (name, value) = line.split_once(':').unwrap();
                (name.to_ascii_lowercase(), value.trim().to_owned())
            })
            .collect();
        parts.push(MultipartPart {
            headers,
            body: &section[separator + 4..],
        });
    }
    parts
}

fn part<'a>(parts: &'a [MultipartPart<'a>], name: &str) -> &'a MultipartPart<'a> {
    parts
        .iter()
        .find(|part| part.disposition_parameter("name") == Some(name))
        .unwrap_or_else(|| panic!("missing multipart field {name}"))
}

fn assert_optional_field(parts: &[MultipartPart<'_>], name: &str, expected: Option<&str>) {
    match expected {
        Some(value) => assert_eq!(part(parts, name).body, value.as_bytes()),
        None => assert!(
            parts
                .iter()
                .all(|part| part.disposition_parameter("name") != Some(name)),
            "field {name} must be absent when not supplied"
        ),
    }
}

fn assert_upload(
    request: &CapturedRequest,
    model: GroqModel,
    language: Option<&str>,
    prompt: Option<&str>,
    filename: &str,
    content_type: &str,
    file: &[u8],
) {
    assert_eq!(
        request.request_line,
        "POST /openai/v1/audio/transcriptions HTTP/1.1"
    );
    assert_eq!(
        request.header("authorization"),
        Some("Bearer test-key-not-secret")
    );
    let content_type_header = request
        .header("content-type")
        .expect("missing multipart content type");
    let boundary = content_type_header
        .strip_prefix("multipart/form-data; boundary=")
        .expect("request must be multipart/form-data with a boundary");
    assert!(!boundary.is_empty());

    let parts = multipart_parts(&request.body, boundary);
    assert_eq!(part(&parts, "model").body, model.as_str().as_bytes());
    assert_eq!(part(&parts, "response_format").body, b"json");
    assert_eq!(part(&parts, "temperature").body, b"0");
    assert_optional_field(&parts, "language", language);
    assert_optional_field(&parts, "prompt", prompt);
    let upload = part(&parts, "file");
    assert_eq!(upload.disposition_parameter("filename"), Some(filename));
    assert_eq!(upload.header("content-type"), Some(content_type));
    assert_eq!(upload.body, file);
    assert!(
        !request
            .body
            .windows(TEST_KEY.len())
            .any(|bytes| bytes == TEST_KEY.as_bytes()),
        "the API key must only appear in Authorization"
    );
}
