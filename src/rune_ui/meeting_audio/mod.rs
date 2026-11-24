//! Meetings › Computer audio's app and output lists, read off the UI thread.
//!
//! Listing runs `pactl` and reads the PipeWire graph on Linux, and Core
//! Audio's process list or COM's audio sessions elsewhere, so the window
//! starts one read when the Settings view shows and adopts its answer in a
//! poll. Until the first answer the row offers all computer audio alone.

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::{Pane, Ui, meetings};
use crate::meeting::audio_source::{self, SourceEntry};

/// The sources a meeting's system track can record, as of the last read.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Sources {
    /// Output devices, the default first.
    pub(super) outputs: Vec<SourceEntry>,
    /// Applications playing now.
    pub(super) apps: Vec<SourceEntry>,
}

/// The row's last read and the one in flight.
#[derive(Default)]
pub(super) struct Listing {
    /// The last finished read, if any.
    pub(super) sources: Option<Sources>,
    read: Option<Receiver<Sources>>,
    /// The poll clock the last read started at.
    pub(super) read_at: f64,
}

impl Listing {
    /// The sources the row offers now.
    pub(super) fn sources(&self) -> Option<&Sources> {
        self.sources.as_ref()
    }
}

/// Starts a read unless one is in flight. A stale source list is kept while
/// the next answer arrives, so the row never flickers.
pub(super) fn refresh(ui: &mut Ui) {
    if !ui.live() || ui.meeting_audio.read.is_some() {
        return;
    }
    let (sender, receiver) = channel();
    let waker = ui.modes.waker.clone();
    let spawned = std::thread::Builder::new()
        .name("hex-meeting-audio".into())
        .spawn(move || {
            let _ = sender.send(Sources {
                outputs: audio_source::output_devices(),
                apps: audio_source::playing_apps(),
            });
            if let Some(waker) = &waker {
                waker();
            }
        });
    if spawned.is_ok() {
        ui.meeting_audio.read = Some(receiver);
    }
}

/// Adopts a finished read and starts the next while Meetings › Settings ›
/// Computer audio shows; true when the adopted list changed what shows.
pub(super) fn poll(ui: &mut Ui, clock: f64) -> bool {
    let changed = adopt(ui);
    if ui.pane == Pane::Meetings
        && ui.meetings.view == meetings::View::Settings
        && ui.settings.labs.meeting_audio_source
        && clock >= ui.meeting_audio.read_at + 3.0
    {
        refresh(ui);
        ui.meeting_audio.read_at = clock;
    }
    changed
}

fn adopt(ui: &mut Ui) -> bool {
    let Some(read) = &ui.meeting_audio.read else {
        return false;
    };
    match read.try_recv() {
        Ok(sources) => {
            let changed = ui.meeting_audio.sources.as_ref() != Some(&sources);
            ui.meeting_audio.sources = Some(sources);
            ui.meeting_audio.read = None;
            changed
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            ui.meeting_audio.read = None;
            false
        }
    }
}
