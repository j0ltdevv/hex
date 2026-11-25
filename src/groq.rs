//! Cloud dictation through Groq. The local transcriber remains Hex's default;
//! this module only runs when the selected provider is Groq and a credential
//! is available. Keys are sent in the Authorization header, never logged.

use std::fmt;
use std::sync::{LazyLock, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use transcribe_cpp::CancelToken;

pub(crate) mod flac;
pub(crate) mod keys;
pub(crate) mod usage;

#[allow(unused_imports)]
pub(crate) use keys::{
    KeyStatus, active_key, key_file_path, key_status, read_key_file, resolve_key, write_key_file,
};
pub(crate) use usage::{UsageOutcome, UsageRecord};

/// A settlement result: `cloud_used` is true only when cloud text was accepted.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Settled<T> {
    pub(crate) value: T,
    pub(crate) cloud_used: bool,
}

/// The closing-mark rule from the selection captured for a whole capture,
/// independent of which local source finishes.
pub(crate) fn closing_mark_required(
    selection: Option<&crate::transcription::models::TranscriptionSelection>,
) -> bool {
    crate::transcription::requires_closing_mark(selection.map(|selection| selection.model))
}

const ENDPOINT: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_AUDIO_BYTES: usize = 24 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Which transcriber a dictation uses. Local is Hex's unchanged default.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechProvider {
    #[default]
    Local,
    Groq,
}

#[allow(dead_code)]
impl SpeechProvider {
    pub const ALL: [Self; 2] = [Self::Local, Self::Groq];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Groq => "groq",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum GroqModel {
    #[default]
    #[serde(rename = "whisper-large-v3")]
    LargeV3,
    #[serde(rename = "whisper-large-v3-turbo")]
    Turbo,
}

impl GroqModel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LargeV3 => "whisper-large-v3",
            Self::Turbo => "whisper-large-v3-turbo",
        }
    }
}

/// An API key whose `Debug` never reveals its bytes.
#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct GroqApiKey(String);

impl GroqApiKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for GroqApiKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[redacted]")
    }
}

impl From<String> for GroqApiKey {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for GroqApiKey {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// One cloud transcription request. `Debug` redacts the prompt and samples.
#[derive(Clone, Copy)]
pub(crate) struct GroqRequest<'a> {
    pub(crate) samples: &'a [f32],
    pub(crate) language: Option<&'a str>,
    pub(crate) prompt: Option<&'a str>,
    pub(crate) model: GroqModel,
}

#[allow(dead_code)]
impl<'a> GroqRequest<'a> {
    pub(crate) fn new(
        samples: &'a [f32],
        language: Option<&'a str>,
        prompt: Option<&'a str>,
        model: GroqModel,
    ) -> Self {
        Self {
            samples,
            language,
            prompt,
            model,
        }
    }

    pub(crate) fn into_owned(self) -> OwnedGroqRequest {
        OwnedGroqRequest {
            samples: self.samples.to_vec(),
            language: self.language.map(str::to_owned),
            prompt: self.prompt.map(str::to_owned),
            model: self.model,
        }
    }
}

impl fmt::Debug for GroqRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroqRequest")
            .field("samples", &self.samples.len())
            .field("language", &self.language.is_some())
            .field("prompt", &self.prompt.is_some())
            .field("model", &self.model)
            .finish()
    }
}

/// An owned request for the worker thread.
#[derive(Clone)]
pub(crate) struct OwnedGroqRequest {
    pub(crate) samples: Vec<f32>,
    pub(crate) language: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) model: GroqModel,
}

impl OwnedGroqRequest {
    pub(crate) fn new(
        samples: Vec<f32>,
        language: Option<String>,
        prompt: Option<String>,
        model: GroqModel,
    ) -> Self {
        Self {
            samples,
            language,
            prompt,
            model,
        }
    }

    pub(crate) fn borrowed(&self) -> GroqRequest<'_> {
        GroqRequest {
            samples: &self.samples,
            language: self.language.as_deref(),
            prompt: self.prompt.as_deref(),
            model: self.model,
        }
    }
}

impl fmt::Debug for OwnedGroqRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.borrowed().fmt(formatter)
    }
}

/// A clonable cancellation flag for a cloud request.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cancellation {
    token: CancelToken,
}

impl Cancellation {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_token(token: CancelToken) -> Self {
        Self { token }
    }

    #[allow(dead_code)]
    pub(crate) fn cancel(&self) {
        self.token.cancel();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

#[derive(Default)]
struct Runtime {
    initialized: bool,
    provider: SpeechProvider,
    model: GroqModel,
    key: Option<GroqApiKey>,
}

static RUNTIME: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::default()));

fn runtime_state() -> MutexGuard<'static, Runtime> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

/// Called by `AppSettings::apply_runtime_values` with the live settings.
pub(crate) fn set_runtime(provider: SpeechProvider, model: GroqModel, key: Option<GroqApiKey>) {
    let mut state = runtime_state();
    state.initialized = true;
    state.provider = provider;
    state.model = model;
    state.key = key;
}

pub(crate) fn speech_provider() -> SpeechProvider {
    runtime_state().provider
}

pub(crate) fn groq_model() -> GroqModel {
    runtime_state().model
}

#[allow(dead_code)]
pub(crate) fn groq_key() -> Option<GroqApiKey> {
    active_key()
}

/// The legacy settings key, from the runtime projection when initialized.
pub(crate) fn saved_key() -> Option<GroqApiKey> {
    let state = runtime_state();
    if state.initialized {
        return state.key.clone();
    }
    drop(state);
    crate::app_settings::AppSettings::read()
        .ok()
        .and_then(|settings| settings.groq_api_key)
}

/// Draft text for the Settings field, without exposing the stored key.
pub(crate) fn draft_text(key: &Option<GroqApiKey>) -> String {
    key.as_ref()
        .map(GroqApiKey::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub(crate) fn draft_key(text: &str, has_saved: bool) -> Option<GroqApiKey> {
    (!text.is_empty() || has_saved).then(|| GroqApiKey::from(text))
}

pub(crate) fn draft_differs(key: &Option<GroqApiKey>, text: &str) -> bool {
    text != draft_text(key)
}

/// Mono 16 kHz, 16-bit PCM for Groq. Non-finite samples become silence.
fn pcm_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&sample| {
            if sample.is_finite() {
                let sample = sample.clamp(-1.0, 1.0);
                let scale = if sample.is_sign_negative() {
                    32768.0
                } else {
                    32767.0
                };
                (sample * scale) as i16
            } else {
                0
            }
        })
        .collect()
}

pub(crate) fn wav(samples: &[f32]) -> Vec<u8> {
    let pcm = pcm_i16(samples);
    let data_len = pcm
        .len()
        .checked_mul(2)
        .expect("dictation is too large for a WAV");
    assert!(
        data_len <= u32::MAX as usize,
        "dictation is too long for a WAV"
    );

    let riff_len = data_len
        .checked_add(36)
        .expect("dictation is too large for a WAV RIFF chunk");
    let riff_len =
        u32::try_from(riff_len).expect("dictation is too large for a 32-bit WAV RIFF chunk");

    let mut header = [0_u8; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&riff_len.to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16_u32.to_le_bytes());
    header[20..22].copy_from_slice(&1_u16.to_le_bytes());
    header[22..24].copy_from_slice(&1_u16.to_le_bytes());
    header[24..28].copy_from_slice(&16_000_u32.to_le_bytes());
    header[28..32].copy_from_slice(&32_000_u32.to_le_bytes());
    header[32..34].copy_from_slice(&2_u16.to_le_bytes());
    header[34..36].copy_from_slice(&16_u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&(data_len as u32).to_le_bytes());

    let mut bytes = Vec::with_capacity(header.len() + data_len);
    bytes.extend_from_slice(&header);
    for sample in pcm {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Prefer a nonempty environment key to a nonempty saved key.
#[allow(dead_code)]
pub(crate) fn key_from(env: Option<&str>, saved: Option<&str>) -> Option<String> {
    env.into_iter()
        .chain(saved)
        .map(str::trim)
        .find(|key| !key.is_empty())
        .map(str::to_owned)
}

/// Use Groq's nonempty text, otherwise run the caller's unchanged local path.
#[allow(dead_code)]
pub(crate) fn prefer_groq(
    groq: Result<String, String>,
    local: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let text = match groq {
        Ok(text) => text,
        Err(_) => return local(),
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return local();
    }
    if trimmed.len() == text.len() {
        Ok(text)
    } else {
        Ok(trimmed.to_owned())
    }
}

#[allow(dead_code)]
pub(crate) fn provider_from(settings: &crate::app_settings::AppSettings) -> SpeechProvider {
    settings.speech_provider
}

#[derive(Clone, Copy)]
enum AudioFormat {
    Flac,
    Wav,
}

/// FLAC first, WAV only when the server rejects FLAC itself.
pub(crate) fn transcribe_with_retry(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &GroqApiKey,
    request: GroqRequest<'_>,
    cancel: &Cancellation,
) -> Result<String, String> {
    let started = Instant::now();
    let model = request.model;
    let samples = request.samples.len();
    let finish = |result: Result<String, String>, outcome: UsageOutcome| {
        usage::record(UsageRecord {
            at_unix_secs: usage::now_unix_secs(),
            provider: SpeechProvider::Groq,
            model,
            outcome,
            samples,
            duration_ms: started.elapsed().as_millis() as u64,
        });
        result
    };

    if cancel.is_cancelled() {
        return finish(Err(cancelled_message()), UsageOutcome::Failed);
    }
    if let Err(failure) = preflight(request.samples) {
        return finish(Err(failure.message()), UsageOutcome::Failed);
    }

    let mut retried = false;
    let result = match transcribe_once(agent, endpoint, key, request, AudioFormat::Flac, cancel) {
        Ok(text) => Ok(text),
        Err(failure) if failure.retryable() => {
            if cancel.is_cancelled() {
                Err(cancelled_message())
            } else {
                retried = true;
                transcribe_once(agent, endpoint, key, request, AudioFormat::Wav, cancel)
                    .map_err(|failure| failure.message())
            }
        }
        Err(failure) => Err(failure.message()),
    };
    let outcome = match (&result, retried) {
        (Ok(_), true) => UsageOutcome::WavRetry,
        (Ok(_), false) => UsageOutcome::Succeeded,
        (Err(_), _) => UsageOutcome::Failed,
    };
    finish(result, outcome)
}

fn preflight(samples: &[f32]) -> Result<(), Failure> {
    if samples.len() > MAX_AUDIO_BYTES / 2 {
        Err(Failure::new("audio", "the recording exceeded 24 MiB"))
    } else {
        Ok(())
    }
}

fn transcribe_once(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &GroqApiKey,
    request: GroqRequest<'_>,
    format: AudioFormat,
    cancel: &Cancellation,
) -> Result<String, Failure> {
    if cancel.is_cancelled() {
        return Err(Failure::new("cancelled", "the transcription was cancelled"));
    }
    let key = key.as_str().trim();
    if key.is_empty() {
        return Err(Failure::new("configuration", "the API key is empty"));
    }

    let audio = match format {
        AudioFormat::Flac => flac::encode(request.samples),
        AudioFormat::Wav => wav(request.samples),
    };
    if audio.len() > MAX_AUDIO_BYTES {
        return Err(Failure::new("audio", "the encoded audio exceeded 24 MiB"));
    }
    let boundary = boundary();
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let body = multipart(
        &boundary,
        request.model,
        request.language,
        request.prompt,
        format,
        &audio,
    );

    let mut response = agent
        .post(endpoint)
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .content_type(&content_type)
        .config()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .send(&body)
        .map_err(|error| Failure::request(endpoint, error))?;

    if cancel.is_cancelled() {
        return Err(Failure::new("cancelled", "the transcription was cancelled"));
    }
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Failure::http(status));
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES + 1)
        .read_to_vec()
        .map_err(|error| Failure::request(endpoint, error))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(Failure::new("response", "the response exceeded 1 MiB"));
    }

    #[derive(Deserialize)]
    struct Transcript {
        text: String,
    }
    let transcript: Transcript = serde_json::from_slice(&bytes)
        .map_err(|_| Failure::new("response", "the response was not transcription JSON"))?;
    if cancel.is_cancelled() {
        return Err(Failure::new("cancelled", "the transcription was cancelled"));
    }
    let trimmed = transcript.text.trim();
    if trimmed.is_empty() {
        return Err(Failure::new("empty", "the transcript was empty"));
    }
    if trimmed.len() == transcript.text.len() {
        Ok(transcript.text)
    } else {
        Ok(trimmed.to_owned())
    }
}

fn multipart(
    boundary: &str,
    model: GroqModel,
    language: Option<&str>,
    prompt: Option<&str>,
    format: AudioFormat,
    audio: &[u8],
) -> Vec<u8> {
    let (filename, content_type) = match format {
        AudioFormat::Flac => ("dictation.flac", "audio/flac"),
        AudioFormat::Wav => ("dictation.wav", "audio/wav"),
    };
    let mut body = Vec::with_capacity(
        256 + boundary.len()
            + model.as_str().len()
            + audio.len()
            + language.map_or(0, str::len)
            + prompt.map_or(0, str::len),
    );
    push_text_field(&mut body, boundary, "model", model.as_str());
    push_text_field(&mut body, boundary, "response_format", "json");
    push_text_field(&mut body, boundary, "temperature", "0");
    if let Some(language) = language.filter(|value| !value.is_empty()) {
        push_text_field(&mut body, boundary, "language", language);
    }
    if let Some(prompt) = prompt.filter(|value| !value.is_empty()) {
        push_text_field(&mut body, boundary, "prompt", prompt);
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"file\"; filename=\"");
    body.extend_from_slice(filename.as_bytes());
    body.extend_from_slice(b"\"\r\nContent-Type: ");
    body.extend_from_slice(content_type.as_bytes());
    body.extend_from_slice(b"\r\n\r\n");
    body.extend_from_slice(audio);
    body.extend_from_slice(b"\r\n--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    body
}

fn push_text_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(b"\"\r\n\r\n");
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

/// A fresh unpredictable boundary per upload: user text can never name the
/// separator that would turn it into extra form fields.
fn boundary() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("system randomness is unavailable");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut boundary = String::with_capacity(9 + bytes.len() * 2);
    boundary.push_str("hex-groq-");
    for byte in bytes {
        boundary.push(HEX[(byte >> 4) as usize] as char);
        boundary.push(HEX[(byte & 0x0f) as usize] as char);
    }
    boundary
}

fn production_agent() -> ureq::Agent {
    crate::net::agent(ENDPOINT, |config| {
        config
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
    })
}

/// Runs one cloud transcription on a worker thread and reports one result.
/// The key is resolved by the caller when the job is submitted, so a later
/// key change cannot redirect or suppress this upload.
pub(crate) fn spawn(
    request: OwnedGroqRequest,
    key: GroqApiKey,
    cancel: CancelToken,
) -> mpsc::Receiver<Result<String, String>> {
    let (sender, receiver) = mpsc::channel();
    let thread_sender = sender.clone();
    let started = thread::Builder::new()
        .name("hex-groq".into())
        .spawn(move || {
            let cancellation = Cancellation::from_token(cancel);
            let result = if cancellation.is_cancelled() {
                Err(cancelled_message())
            } else {
                transcribe_with_retry(
                    &production_agent(),
                    ENDPOINT,
                    &key,
                    request.borrowed(),
                    &cancellation,
                )
            };
            if let Err(error) = &result {
                tracing::warn!(
                    target: "hex_config",
                    host = crate::net::host(ENDPOINT),
                    %error,
                    "Groq transcription failed"
                );
            }
            let _ = thread_sender.send(result);
        });
    if let Err(error) = started {
        let _ = sender.send(Err(format!("Groq transcription failed (thread): {error}")));
    }
    receiver
}

/// One in-flight cloud result beside a local transcript. The caller waits for
/// it in job order, so a slow request never overtakes a later paste.
pub(crate) struct PendingGroq {
    reply: mpsc::Receiver<Result<String, String>>,
    cancel: CancelToken,
    closing_mark_required: bool,
}

impl PendingGroq {
    /// Snapshots the credential before spawning; with no key there is no
    /// cloud thread and no upload.
    pub(crate) fn start(
        request: Option<OwnedGroqRequest>,
        cancel: CancelToken,
        closing_mark_required: bool,
    ) -> Option<Self> {
        let key = request.is_some().then(active_key).flatten();
        Self::start_with_key(request, key, cancel, closing_mark_required)
    }

    fn start_with_key(
        request: Option<OwnedGroqRequest>,
        key: Option<GroqApiKey>,
        cancel: CancelToken,
        closing_mark_required: bool,
    ) -> Option<Self> {
        let request = request?;
        let key = key?;
        Some(Self {
            reply: spawn(request, key, cancel.clone()),
            cancel,
            closing_mark_required,
        })
    }

    /// The closing-mark rule captured with this job, used when settling
    /// cloud text beside a failed local run.
    pub(crate) fn closing_mark_required(&self) -> bool {
        self.closing_mark_required
    }

    /// Returns cloud text only when accepted. Any error, rejection,
    /// disconnect, or cancellation keeps the local result.
    pub(crate) fn settle(
        self,
        local_text: &str,
        accept: impl FnOnce(String) -> Result<String, String>,
    ) -> Settled<String> {
        match self.settle_cloud(accept) {
            Some(text) => Settled {
                value: text,
                cloud_used: true,
            },
            None => Settled {
                value: local_text.to_owned(),
                cloud_used: false,
            },
        }
    }

    /// Settles with no local text, for a local transcription that failed.
    pub(crate) fn settle_cloud(
        self,
        accept: impl FnOnce(String) -> Result<String, String>,
    ) -> Option<String> {
        Self::receive(&self.reply, &self.cancel, accept)
    }

    fn receive(
        reply: &mpsc::Receiver<Result<String, String>>,
        cancel: &CancelToken,
        accept: impl FnOnce(String) -> Result<String, String>,
    ) -> Option<String> {
        loop {
            match reply.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(text)) => {
                    // A cancel can land after the worker queued its text.
                    if cancel.is_cancelled() {
                        return None;
                    }
                    return accept(text).ok();
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(mpsc::RecvTimeoutError::Timeout) if cancel.is_cancelled() => return None,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

impl Drop for PendingGroq {
    fn drop(&mut self) {
        // A rejected, abandoned, or settled job must not leave an upload
        // running; the flag is also observed inside the worker.
        self.cancel.cancel();
    }
}

/// Settles an optional cloud job beside a local result. An already-started
/// cloud request is also consumed (or cancelled) when the local run failed.
pub(crate) fn settle_local<E>(
    pending: Option<PendingGroq>,
    local: Result<String, E>,
    accept: impl FnOnce(String) -> Result<String, String>,
) -> Settled<Result<String, E>> {
    match local {
        Ok(text) => {
            let settled = settle_option(pending, &text, accept);
            Settled {
                value: Ok(settled.value),
                cloud_used: settled.cloud_used,
            }
        }
        Err(error) => match pending.and_then(|pending| pending.settle_cloud(accept)) {
            Some(text) => Settled {
                value: Ok(text),
                cloud_used: true,
            },
            None => Settled {
                value: Err(error),
                cloud_used: false,
            },
        },
    }
}

/// Settles an optional cloud job, leaving the local text untouched when no
/// request was started.
pub(crate) fn settle_option(
    pending: Option<PendingGroq>,
    local_text: &str,
    accept: impl FnOnce(String) -> Result<String, String>,
) -> Settled<String> {
    pending.map_or_else(
        || Settled {
            value: local_text.to_owned(),
            cloud_used: false,
        },
        |pending| pending.settle(local_text, accept),
    )
}

/// Settlement plus the local source label and timing a dictation worker logs.
pub(crate) struct TranscriptSettlement<T> {
    pub(crate) value: T,
    pub(crate) cloud_used: bool,
    pub(crate) source: String,
    pub(crate) finish_ms: u64,
}

/// Settles a whole local transcript and its optional cloud request. The
/// closing-mark rule is read from the pending job, so it also applies when
/// the local stream falls back or fails.
pub(crate) fn settle_transcript<E>(
    pending: Option<PendingGroq>,
    local: Result<crate::transcription::engine::Transcribed, E>,
    accept: fn(String, bool) -> Result<String, String>,
) -> TranscriptSettlement<Result<String, E>> {
    let source = local.as_ref().map_or_else(
        |_| "unavailable".to_owned(),
        |transcribed| transcribed.source.label().to_owned(),
    );
    let finish_ms = local
        .as_ref()
        .map_or(0, |transcribed| transcribed.finish_ms);
    let required = pending
        .as_ref()
        .is_some_and(PendingGroq::closing_mark_required);
    let settled = settle_local(pending, local.map(|transcribed| transcribed.text), |text| {
        accept(text, required)
    });
    TranscriptSettlement {
        value: settled.value,
        cloud_used: settled.cloud_used,
        source,
        finish_ms,
    }
}

/// Compatibility path for callers not yet routed through `spawn`.
#[allow(dead_code)]
pub(crate) fn transcribe_if_configured(samples: &[f32]) -> Result<String, String> {
    let settings = crate::app_settings::AppSettings::read().unwrap_or_default();
    if provider_from(&settings) != SpeechProvider::Groq {
        return Err("Groq is not selected".into());
    }
    let Some(key) = active_key() else {
        return Err("Groq is not configured".into());
    };
    let result = transcribe_with_retry(
        &production_agent(),
        ENDPOINT,
        &key,
        GroqRequest::new(samples, None, None, settings.groq_model),
        &Cancellation::new(),
    );
    if let Err(error) = &result {
        tracing::warn!(
            target: "hex_config",
            host = crate::net::host(ENDPOINT),
            %error,
            "Groq transcription failed"
        );
    }
    result
}

#[cfg(test)]
pub(crate) fn transcribe_with(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &str,
    model: GroqModel,
    samples: &[f32],
) -> Result<String, String> {
    let key = GroqApiKey::from(key);
    let request = GroqRequest::new(samples, None, None, model);
    transcribe_once(
        agent,
        endpoint,
        &key,
        request,
        AudioFormat::Wav,
        &Cancellation::new(),
    )
    .map_err(|failure| failure.message())
}

fn cancelled_message() -> String {
    "Groq transcription was cancelled".into()
}

struct Failure {
    class: &'static str,
    reason: String,
    status: Option<u16>,
}

impl Failure {
    fn new(class: &'static str, reason: impl Into<String>) -> Self {
        Self {
            class,
            reason: reason.into(),
            status: None,
        }
    }

    fn http(status: u16) -> Self {
        Self {
            class: "http",
            reason: format!("the server answered HTTP {status}"),
            status: Some(status),
        }
    }

    fn retryable(&self) -> bool {
        // Only audio-format rejections can be fixed by re-encoding as WAV;
        // a 400 for a bad prompt/model/language must not be billed twice.
        matches!(self.status, Some(415 | 422))
    }

    fn request(endpoint: &str, error: ureq::Error) -> Self {
        let reason = match &error {
            ureq::Error::BadUri(_) => "the endpoint is invalid".to_owned(),
            ureq::Error::Http(_) => "the request was invalid".to_owned(),
            ureq::Error::Timeout(_) => "the request timed out".to_owned(),
            ureq::Error::BodyExceedsLimit(_) => "the response exceeded 1 MiB".to_owned(),
            ureq::Error::StatusCode(status) => format!("the server answered HTTP {status}"),
            _ => crate::net::unreachable(endpoint, &error),
        };
        Self {
            class: "transport",
            reason,
            status: match error {
                ureq::Error::StatusCode(status) => Some(status),
                _ => None,
            },
        }
    }

    fn message(&self) -> String {
        format!(
            "Groq transcription failed ({}): {}",
            self.class, self.reason
        )
    }
}

#[cfg(test)]
mod tests;
