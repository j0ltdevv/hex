//! Cloud dictation through Groq, with the local transcriber as the fallback.
//! The API key is read from the environment or Hex's owner-only settings and
//! is sent only in the request's Authorization header.

use std::time::Duration;

use serde::{Deserialize, Serialize};

const ENDPOINT: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const BOUNDARY: &str = "hex-groq-dictation";

const MULTIPART_START: &[u8] =
    b"--hex-groq-dictation\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n";
const MULTIPART_MIDDLE: &[u8] = b"\r\n--hex-groq-dictation\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\njson\r\n--hex-groq-dictation\r\nContent-Disposition: form-data; name=\"temperature\"\r\n\r\n0\r\n--hex-groq-dictation\r\nContent-Disposition: form-data; name=\"file\"; filename=\"dictation.wav\"\r\nContent-Type: audio/wav\r\n\r\n";
const MULTIPART_END: &[u8] = b"\r\n--hex-groq-dictation--\r\n";

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
pub(crate) fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples
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
    for &sample in samples {
        let sample = if sample.is_finite() {
            let sample = sample.clamp(-1.0, 1.0);
            let scale = if sample.is_sign_negative() {
                32768.0
            } else {
                32767.0
            };
            (sample * scale) as i16
        } else {
            0
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Prefer a nonempty environment key to a nonempty saved key.
pub(crate) fn key_from(env: Option<&str>, saved: Option<&str>) -> Option<String> {
    env.into_iter()
        .chain(saved)
        .map(str::trim)
        .find(|key| !key.is_empty())
        .map(str::to_owned)
}

/// Use Groq's nonempty text, otherwise run the caller's unchanged local path.
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

#[cfg(test)]
pub(crate) fn transcribe_with(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &str,
    model: GroqModel,
    samples: &[f32],
) -> Result<String, String> {
    request(agent, endpoint, key, model, samples).map_err(|failure| failure.message())
}

/// Cloud-first dictation when a key is configured. Every failure is an error
/// so the caller can execute its existing local transcription path.
pub(crate) fn transcribe_if_configured(samples: &[f32]) -> Result<String, String> {
    let settings = crate::app_settings::AppSettings::read().unwrap_or_default();
    let env = std::env::var("GROQ_API_KEY").ok();
    let saved = settings.groq_api_key.as_ref().map(GroqApiKey::as_str);
    let Some(key) = key_from(env.as_deref(), saved) else {
        return Err("Groq is not configured".into());
    };
    let agent = crate::net::agent(ENDPOINT, |config| {
        config
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
    });
    request(&agent, ENDPOINT, &key, settings.groq_model, samples).map_err(|failure| {
        tracing::warn!(
            target: "hex_config",
            host = crate::net::host(ENDPOINT),
            reason = failure.reason.as_str(),
            error_class = failure.class,
            "Groq transcription failed"
        );
        failure.message()
    })
}

fn request(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &str,
    model: GroqModel,
    samples: &[f32],
) -> Result<String, Failure> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Failure::new("configuration", "the API key is empty"));
    }
    let wav = wav(samples);
    let body = multipart(model.as_str(), &wav);
    let mut response = agent
        .post(endpoint)
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .content_type(format!("multipart/form-data; boundary={BOUNDARY}"))
        .config()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .send(&body)
        .map_err(|error| Failure::request(endpoint, error))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Failure::new(
            "http",
            format!("the server answered HTTP {status}"),
        ));
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
    if transcript.text.trim().is_empty() {
        return Err(Failure::new("empty", "the transcript was empty"));
    }
    Ok(transcript.text)
}

fn multipart(model: &str, wav: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(
        MULTIPART_START.len()
            + model.len()
            + MULTIPART_MIDDLE.len()
            + wav.len()
            + MULTIPART_END.len(),
    );
    body.extend_from_slice(MULTIPART_START);
    body.extend_from_slice(model.as_bytes());
    body.extend_from_slice(MULTIPART_MIDDLE);
    body.extend_from_slice(wav);
    body.extend_from_slice(MULTIPART_END);
    body
}

struct Failure {
    class: &'static str,
    reason: String,
}

impl Failure {
    fn new(class: &'static str, reason: impl Into<String>) -> Self {
        Self {
            class,
            reason: reason.into(),
        }
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
        Self::new("transport", reason)
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
