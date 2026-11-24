//! Small, secret-free record of recent cloud transcription attempts.
//!
//! Records stay in memory for this process. They never hold credentials,
//! multipart bytes, audio, prompts, languages, or transcripts, so a debug
//! dump of this module's state cannot leak dictation content.

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::GroqModel;

const CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum UsageOutcome {
    #[default]
    Succeeded,
    WavRetry,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UsageRecord {
    pub(crate) at_unix_secs: u64,
    pub(crate) provider: super::SpeechProvider,
    pub(crate) model: GroqModel,
    pub(crate) outcome: UsageOutcome,
    pub(crate) samples: usize,
    pub(crate) duration_ms: u64,
}

static RECORDS: LazyLock<Mutex<VecDeque<UsageRecord>>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(CAPACITY)));

fn records() -> &'static Mutex<VecDeque<UsageRecord>> {
    &RECORDS
}

pub(crate) fn record(record: UsageRecord) {
    let mut records = records().lock().unwrap_or_else(|error| error.into_inner());
    if records.len() == CAPACITY {
        records.pop_front();
    }
    records.push_back(record);
}

/// Most recent records first.
#[allow(dead_code)]
pub(crate) fn snapshot() -> Vec<UsageRecord> {
    let records = records().lock().unwrap_or_else(|error| error.into_inner());
    records.iter().rev().copied().collect()
}

pub(crate) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
