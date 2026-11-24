use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use super::*;

const TEST_KEY: &str = "test-key-not-secret";
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
fn prefer_groq_uses_trimmed_nonempty_text_without_running_local() {
    let mut local_calls = 0;

    let result = prefer_groq(Ok(" \n Groq text\t ".to_owned()), || {
        local_calls += 1;
        Ok("local text".to_owned())
    });

    assert_eq!(result, Ok("Groq text".to_owned()));
    assert_eq!(local_calls, 0);
}

#[test]
fn prefer_groq_runs_local_once_for_error_empty_or_whitespace_text() {
    for (case, groq) in [
        ("error", Err("groq unavailable".to_owned())),
        ("empty", Ok(String::new())),
        ("whitespace", Ok(" \t\n".to_owned())),
    ] {
        let mut local_calls = 0;
        let result = prefer_groq(groq.clone(), || {
            local_calls += 1;
            Ok(" local text\n".to_owned())
        });

        assert_eq!(result, Ok(" local text\n".to_owned()), "{case}");
        assert_eq!(local_calls, 1, "{case}");

        let mut local_calls = 0;
        let result = prefer_groq(groq, || {
            local_calls += 1;
            Err("local unavailable".to_owned())
        });

        assert_eq!(result, Err("local unavailable".to_owned()), "{case}");
        assert_eq!(local_calls, 1, "{case}");
    }
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
    let key = key.trim();
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
    let text = transcribe_with(&agent, ENDPOINT, key, GroqModel::LargeV3, &samples)
        .expect("live Groq transcription failed");

    assert!(!text.trim().is_empty());
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
fn key_prefers_trimmed_environment_and_falls_back_to_trimmed_saved_key() {
    assert_eq!(
        key_from(Some("  env-key \n"), Some("saved-key")).as_deref(),
        Some("env-key")
    );
    assert_eq!(
        key_from(Some("env-key"), Some("  saved-key  ")).as_deref(),
        Some("env-key")
    );
    assert_eq!(
        key_from(Some(""), Some("  saved-key \n")).as_deref(),
        Some("saved-key")
    );
    assert_eq!(
        key_from(Some(" \t\n"), Some(" saved-key ")).as_deref(),
        Some("saved-key")
    );
    assert_eq!(
        key_from(None, Some(" saved-key ")).as_deref(),
        Some("saved-key")
    );
    assert_eq!(key_from(None, None), None);
    assert_eq!(key_from(Some(""), None), None);
    assert_eq!(key_from(Some(" \t"), Some(" \n")), None);
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
fn transcribe_posts_the_expected_multipart_and_parses_json_text() {
    let (endpoint, server) = mock_server(
        "200 OK",
        r#"{"text":"Bonjour, \"Groq\" !\n\u00e7a va","language":"fr"}"#,
    );
    let agent = crate::net::loopback_agent(|config| config);

    let text = transcribe_with(&agent, &endpoint, TEST_KEY, GroqModel::Turbo, SAMPLES).unwrap();

    assert_eq!(text, "Bonjour, \"Groq\" !\nça va");
    let request = server.join().unwrap();
    assert_upload(&request, GroqModel::Turbo);
}

#[test]
fn transcribe_returns_an_error_for_unauthorized_responses() {
    let (endpoint, server) = mock_server(
        "401 Unauthorized",
        r#"{"error":{"message":"invalid api key"}}"#,
    );
    let agent = crate::net::loopback_agent(|config| config);

    let error =
        transcribe_with(&agent, &endpoint, TEST_KEY, GroqModel::LargeV3, SAMPLES).unwrap_err();

    assert!(!error.trim().is_empty());
    assert!(!error.contains(TEST_KEY));
    let request = server.join().unwrap();
    assert_upload(&request, GroqModel::LargeV3);
}

#[test]
fn transcribe_returns_an_error_for_server_failures() {
    let (endpoint, server) = mock_server(
        "500 Internal Server Error",
        r#"{"error":{"message":"service unavailable"}}"#,
    );
    let agent = crate::net::loopback_agent(|config| config);

    let error =
        transcribe_with(&agent, &endpoint, TEST_KEY, GroqModel::LargeV3, SAMPLES).unwrap_err();

    assert!(!error.trim().is_empty());
    assert!(!error.contains(TEST_KEY));
    let request = server.join().unwrap();
    assert_upload(&request, GroqModel::LargeV3);
}

#[test]
fn transcribe_rejects_malformed_or_empty_json_text() {
    for response in ["not json", "{}", r#"{"text":""}"#, r#"{"text":"  "}"#] {
        let (endpoint, server) = mock_server("200 OK", response);
        let agent = crate::net::loopback_agent(|config| config);

        let error =
            transcribe_with(&agent, &endpoint, TEST_KEY, GroqModel::LargeV3, SAMPLES).unwrap_err();

        assert!(!error.trim().is_empty(), "response was {response}");
        assert!(!error.contains(TEST_KEY));
        server.join().unwrap();
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
    status: &'static str,
    response: &'static str,
) -> (String, thread::JoinHandle<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
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
    });
    (
        format!("http://{address}/openai/v1/audio/transcriptions"),
        server,
    )
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

fn assert_upload(request: &CapturedRequest, model: GroqModel) {
    assert_eq!(
        request.request_line,
        "POST /openai/v1/audio/transcriptions HTTP/1.1"
    );
    assert_eq!(
        request.header("authorization"),
        Some("Bearer test-key-not-secret")
    );
    let content_type = request
        .header("content-type")
        .expect("missing multipart content type");
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("request must be multipart/form-data with a boundary");
    assert!(!boundary.is_empty());

    let parts = multipart_parts(&request.body, boundary);
    assert_eq!(part(&parts, "model").body, model.as_str().as_bytes());
    assert_eq!(part(&parts, "response_format").body, b"json");
    assert_eq!(part(&parts, "temperature").body, b"0");
    let file = part(&parts, "file");
    assert_eq!(
        file.disposition_parameter("filename"),
        Some("dictation.wav")
    );
    assert_eq!(file.header("content-type"), Some("audio/wav"));
    assert_eq!(file.body, wav(SAMPLES));
    assert!(
        !request
            .body
            .windows(TEST_KEY.len())
            .any(|bytes| bytes == TEST_KEY.as_bytes()),
        "the API key must only appear in Authorization"
    );
}
