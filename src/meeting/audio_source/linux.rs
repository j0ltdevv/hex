//! Linux's meeting audio sources: PipeWire playback apps and the sound
//! server's sinks. Both reuse the readers the rest of Hex already runs (one
//! `pw-dump --monitor` graph and `pactl`), so listing starts no new kind of
//! process.

use super::SourceEntry;

/// The applications with a running output stream now, from the PipeWire
/// graph. The id is the executable (else the application name) the capture
/// backend resolves its nodes by.
pub(super) fn playing_apps() -> Vec<SourceEntry> {
    crate::microphone_activity::playing_applications()
        .into_iter()
        .map(|app| SourceEntry {
            id: app.id,
            name: app.name,
        })
        .collect()
}

/// The sinks a meeting can record, the default first, with the sound
/// server's own description.
pub(super) fn output_devices() -> Vec<SourceEntry> {
    crate::audio::output_route::meeting_output_devices()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, name)| SourceEntry { id, name })
        .collect()
}
