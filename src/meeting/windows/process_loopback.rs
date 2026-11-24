//! WASAPI process loopback: one app's playback for a meeting's system track
//! (Labs › Choose meeting audio).
//!
//! ActivateAudioInterfaceAsync on VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK opens
//! a loopback of a process tree's audio, on whatever outputs it plays,
//! without hearing anything else. Windows 10 version 2004 (build 19041) added
//! it; before that the activation fails, which is logged once and recorded as
//! silence rather than falling back to all computer audio.
//!
//! One capture thread per process-tree root owns its COM objects from
//! CoInitializeEx through drop, so a closed app releases them where they were
//! made, and fills a bounded channel with its audio. The source mixes the
//! roots' audio and re-lists the app's processes on the same 2 s cadence as
//! the rest of the Windows capture, reopening when that set changes. An app
//! that is not running records silence, and is picked up when it starts; an
//! app that is running but whose roots would not open fails the open, so the
//! supervisor retries it with its bounded backoff.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use color_eyre::eyre::{Result, WrapErr, eyre};
use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, WAIT_FAILED};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
    IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    WAVE_FORMAT_PCM, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    BLOB, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, IAgileObject, IAgileObject_Impl,
    StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0},
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{HRESULT, IUnknown, Interface, PCWSTR, Ref, implement};

use super::super::SAMPLE_RATE;
use super::super::audio_source::SourceChoice;
use super::super::capture::{LiveSource, MIX_HOLD, POLL_INTERVAL, PacketSink, To16k, mix};

/// Audio buffered for the source, however many roots the app has.
const QUEUE_CAPACITY: usize = 512;
/// How often the source checks that the app's processes have not changed.
const ROUTE_CHECK: Duration = Duration::from_secs(2);
/// How long one ActivateAudioInterfaceAsync may take to complete.
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(3);
/// How long opening one root, activation included, may take.
const OPEN_TIMEOUT: Duration = Duration::from_secs(6);
/// The stream's buffer, and shared mode's periodicity.
const BUFFER_HNS: i64 = 200_000;
const PERIODICITY_HNS: i64 = 0;
/// How long the capture thread waits for a packet before checking its stop.
const EVENT_WAIT_MS: u32 = 100;
const CHANNELS: u16 = 2;
const BITS_PER_SAMPLE: u16 = 16;
/// The rates requested, in order: process loopback has no mix format to read,
/// so the stream is opened with a plain 16-bit stereo format the engine
/// converts into, and 44.1 kHz stands in for a device that refuses 48 kHz.
const RATES: [u32; 2] = [48_000, 44_100];

/// The system track's audio from one app, through WASAPI process loopback.
pub(super) struct AppSource {
    /// The app's executable file name, lowercased such as ms-teams.exe.
    id: String,
    /// The choice this source opened with, so a different one reopens it:
    /// a change applies mid-recording.
    choice: SourceChoice,
    /// The process-tree roots this source opened, as listed.
    roots: Vec<u32>,
    /// One capture thread per root, in open order. An entry stays here after
    /// its stream ends, so every stream's index keeps naming its root.
    captures: Vec<Capture>,
    /// Kept while no capture runs, so the channel never disconnects.
    sender: SyncSender<(usize, Vec<f32>)>,
    buffers: Receiver<(usize, Vec<f32>)>,
    errors: Receiver<(usize, String)>,
    route_checked: Instant,
}

impl AppSource {
    /// Opens one process loopback per root of the chosen app, or silence when
    /// it is not running. Roots that exist but would not open at all fail the
    /// open, so the supervisor retries with its bounded backoff; the choice
    /// never widens to every output either way.
    pub(super) fn start(choice: SourceChoice) -> Result<Self> {
        let Some(id) = choice.app_id().map(str::to_owned) else {
            return Err(eyre!("the meeting audio choice is not an app"));
        };
        let roots = root_processes(&id).unwrap_or_default();
        let (sender, buffers) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (error_sender, errors) = mpsc::sync_channel(roots.len().max(1));
        let mut captures = Vec::with_capacity(roots.len());
        let mut first_error = None;
        for &pid in &roots {
            match Capture::start(pid, captures.len(), sender.clone(), error_sender.clone()) {
                Ok(capture) => captures.push(capture),
                Err(error) => {
                    tracing::debug!(
                        pid,
                        error = %format!("{error:#}"),
                        "could not hear one process of the chosen app"
                    );
                    first_error.get_or_insert(error);
                }
            }
        }
        if roots.is_empty() {
            tracing::info!(
                app = id,
                "the chosen app isn't running; recording silence until it starts"
            );
        } else if captures.is_empty() {
            // Windows before 10 2004 refuses the activation, and any other
            // failure fails this open: the supervisor retries it on its
            // bounded backoff, and the choice never widens to every output.
            let error = first_error
                .map(|error| format!("{error:#}"))
                .unwrap_or_else(|| "no process could be captured".into());
            tracing::warn!(
                app = id,
                %error,
                "could not capture the chosen app's audio; retrying"
            );
            return Err(eyre!("could not capture {id}: {error}"));
        } else {
            tracing::info!(
                app = id,
                pids = ?captures.iter().map(|capture| capture.pid).collect::<Vec<_>>(),
                "the chosen app's audio loopback started"
            );
        }
        Ok(Self {
            id,
            choice,
            roots,
            captures,
            sender,
            buffers,
            errors,
            route_checked: Instant::now(),
        })
    }

    /// Why this source should reopen on a route check: the app's process-tree
    /// roots changed. A listing that cannot be read keeps them.
    fn route_change(&self) -> Option<&'static str> {
        let roots = root_processes(&self.id)?;
        (roots != self.roots).then_some("the app's processes changed")
    }

    fn take(&mut self, (index, samples): (usize, Vec<f32>)) {
        if let Some(capture) = self.captures.get_mut(index)
            && !capture.ended
        {
            let resampled = capture.resample.push(&samples);
            capture.pending.extend(resampled);
        }
    }
}

impl LiveSource for AppSource {
    fn poll(&mut self, sink: &PacketSink) -> std::result::Result<(), String> {
        // A failing root stops alone; the others keep the track going, and
        // the source reopens once no root is left that the app still has.
        while let Ok((index, error)) = self.errors.try_recv() {
            if let Some(capture) = self.captures.get_mut(index)
                && !capture.ended
            {
                tracing::warn!(pid = capture.pid, %error, "the app's audio loopback stopped");
                capture.end();
            }
        }
        if !self.roots.is_empty() && self.captures.iter().all(|capture| capture.ended) {
            return Err("every app audio loopback stopped".into());
        }
        if self.route_checked.elapsed() >= ROUTE_CHECK {
            self.route_checked = Instant::now();
            if SourceChoice::effective() != self.choice {
                return Err("the meeting audio choice changed".into());
            }
            if let Some(reason) = self.route_change() {
                return Err(reason.into());
            }
        }
        match self.buffers.recv_timeout(POLL_INTERVAL) {
            Ok(first) => {
                self.take(first);
                while let Ok(next) = self.buffers.try_recv() {
                    self.take(next);
                }
                let mut pending = self
                    .captures
                    .iter_mut()
                    .filter(|capture| !capture.ended)
                    .map(|capture| &mut capture.pending)
                    .collect::<Vec<_>>();
                sink.send(SAMPLE_RATE, mix(&mut pending, MIX_HOLD), None);
                Ok(())
            }
            // The app is not running, or plays nothing: silence, padded by
            // the track writer like any other silent output.
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("the app's audio loopback disconnected".into())
            }
        }
    }

    fn describe(&self) -> &'static str {
        "wasapi-process-loopback"
    }
}

/// One root's process loopback stream and the audio it has not been mixed.
struct Capture {
    pid: u32,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    resample: To16k,
    pending: Vec<f32>,
    /// Whether its stream ended and its worker has been joined.
    ended: bool,
}

impl Capture {
    /// Opens the pid's process loopback on its own thread, returning it once
    /// the stream runs.
    fn start(
        pid: u32,
        index: usize,
        sender: SyncSender<(usize, Vec<f32>)>,
        errors: SyncSender<(usize, String)>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready, started) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("meeting-app-loopback".into())
            .spawn(move || capture(pid, index, sender, errors, &thread_stop, ready))
            .wrap_err("could not start the app loopback thread")?;
        let rate = match started.recv_timeout(OPEN_TIMEOUT) {
            Ok(Ok(rate)) => rate,
            Ok(Err(error)) => {
                stop.store(true, Ordering::Relaxed);
                return Err(eyre!(error));
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                return Err(eyre!("the app's loopback stream did not start in time"));
            }
        };
        let resample = To16k::new(rate)?;
        Ok(Self {
            pid,
            stop,
            worker: Some(worker),
            resample,
            pending: Vec::new(),
            ended: false,
        })
    }

    /// Ends this capture, joining its thread so nothing outlives the source.
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.pending.clear();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("app audio loopback thread panicked");
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.end();
    }
}

/// Runs one root's process loopback: activation, initialization, and its
/// packets, all in this thread's COM apartment.
fn capture(
    pid: u32,
    index: usize,
    sender: SyncSender<(usize, Vec<f32>)>,
    errors: SyncSender<(usize, String)>,
    stop: &AtomicBool,
    ready: SyncSender<std::result::Result<u32, String>>,
) {
    // SAFETY: Joins this thread to the multithreaded apartment. A thread
    // already in it gets S_FALSE, which still needs the CoUninitialize below.
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    run_capture(pid, index, &sender, &errors, stop, &ready);
    // Every COM object this thread made is dropped by now.
    if initialized {
        // SAFETY: Balances the successful CoInitializeEx above on this thread,
        // and this thread keeps running nothing else.
        unsafe { CoUninitialize() };
    }
}

/// Records one root until it stops or fails, dropping its COM objects before
/// it returns.
fn run_capture(
    pid: u32,
    index: usize,
    sender: &SyncSender<(usize, Vec<f32>)>,
    errors: &SyncSender<(usize, String)>,
    stop: &AtomicBool,
    ready: &SyncSender<std::result::Result<u32, String>>,
) {
    let opened = match open(pid) {
        Ok(opened) => opened,
        Err(error) => {
            let _ = ready.try_send(Err(error));
            return;
        }
    };
    if ready.send(Ok(opened.format.nSamplesPerSec)).is_err() {
        return;
    }
    if let Err(error) = pump(
        &opened.capture,
        &opened.format,
        opened.event.0,
        index,
        sender,
        stop,
    ) {
        let _ = errors.try_send((index, error));
    }
    // SAFETY: Stops this thread's own initialized stream before it drops.
    let _ = unsafe { opened.client.Stop() };
}

/// A root's open stream: its client, its capture service, the event its
/// packets are waited on, and the format they arrive in.
struct Opened {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: OwnedEvent,
    format: WAVEFORMATEX,
}

/// Opens process loopback of the pid and starts it, trying each requested
/// rate.
fn open(pid: u32) -> std::result::Result<Opened, String> {
    let mut last = "the process could not be captured".to_owned();
    for rate in RATES {
        let format = pcm_format(rate);
        let client = match activate(pid) {
            Ok(client) => client,
            Err(error) => {
                last = error;
                continue;
            }
        };
        // SAFETY: The format is a valid WAVEFORMATEX for the call, and the
        // client is this thread's own, uninitialized one.
        let initialized = unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    // The app may play a rate or channel count the requested
                    // format does not name: the engine converts it instead of
                    // refusing the stream.
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                BUFFER_HNS,
                PERIODICITY_HNS,
                &format,
                None,
            )
        };
        if let Err(error) = initialized {
            last = hresult("Initialize", &error);
            continue;
        }
        // SAFETY: A fresh event owned by OwnedEvent, closed on drop.
        let event = match unsafe { CreateEventW(None, false, false, PCWSTR::null()) } {
            Ok(event) => OwnedEvent(event),
            Err(error) => {
                last = hresult("CreateEvent", &error);
                continue;
            }
        };
        // SAFETY: The event outlives the stream, and no second handle is set.
        if let Err(error) = unsafe { client.SetEventHandle(event.0) } {
            last = hresult("SetEventHandle", &error);
            continue;
        }
        // SAFETY: The client was initialized, so it serves a capture client.
        let capture = match unsafe { client.GetService::<IAudioCaptureClient>() } {
            Ok(capture) => capture,
            Err(error) => {
                last = hresult("GetService", &error);
                continue;
            }
        };
        // SAFETY: The stream was initialized, so it can start.
        if let Err(error) = unsafe { client.Start() } {
            last = hresult("Start", &error);
            continue;
        }
        return Ok(Opened {
            client,
            capture,
            event,
            format,
        });
    }
    Err(last)
}

/// Delivers the stream's packets to the sender until stop or a failure.
fn pump(
    capture: &IAudioCaptureClient,
    format: &WAVEFORMATEX,
    event: HANDLE,
    index: usize,
    sender: &SyncSender<(usize, Vec<f32>)>,
    stop: &AtomicBool,
) -> std::result::Result<(), String> {
    let block = usize::from(format.nBlockAlign);
    let channels = usize::from(format.nChannels);
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: Waits on this stream's own event, with a bounded timeout so
        // a stop is noticed without one.
        if unsafe { WaitForSingleObject(event, EVENT_WAIT_MS) } == WAIT_FAILED {
            return Err("the app's audio loopback event failed".into());
        }
        loop {
            // SAFETY: The client belongs to this thread; the sizes it returns
            // are used as given.
            let packet = unsafe { capture.GetNextPacketSize() }
                .map_err(|error| hresult("GetNextPacketSize", &error))?;
            if packet == 0 {
                break;
            }
            let mut data = std::ptr::null_mut();
            let mut frames = 0_u32;
            let mut flags = 0_u32;
            unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
                .map_err(|error| hresult("GetBuffer", &error))?;
            let frames_count = frames as usize;
            let samples = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                vec![0.0; frames_count]
            } else {
                // SAFETY: GetBuffer returned that many frames of block bytes.
                let bytes = unsafe { std::slice::from_raw_parts(data, frames_count * block) };
                mono_i16(bytes, channels)
            };
            unsafe { capture.ReleaseBuffer(frames) }
                .map_err(|error| hresult("ReleaseBuffer", &error))?;
            if samples.is_empty() {
                continue;
            }
            // A full queue drops this root's buffer rather than growing
            // without bound; the mix keeps the others.
            let _ = sender.try_send((index, samples));
        }
    }
    Ok(())
}

/// The WAVEFORMATEX process loopback is opened with: a plain 16-bit stereo
/// PCM format, which the audio engine converts the app's audio into.
fn pcm_format(rate: u32) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8),
        nBlockAlign: CHANNELS * (BITS_PER_SAMPLE / 8),
        wBitsPerSample: BITS_PER_SAMPLE,
        cbSize: 0,
    }
}

/// Interleaved 16-bit little-endian frames, averaged to the mono f32 the
/// meeting tracks carry. A trailing partial frame is dropped.
fn mono_i16(bytes: &[u8], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let (samples, _) = bytes.as_chunks::<2>();
    samples
        .chunks_exact(channels)
        .map(|frame| {
            frame
                .iter()
                .map(|sample| f32::from(i16::from_le_bytes(*sample)) / f32::from(i16::MAX))
                .sum::<f32>()
                / frame.len() as f32
        })
        .collect()
}

/// Activates a process loopback IAudioClient for the pid and waits for it.
fn activate(pid: u32) -> std::result::Result<IAudioClient, String> {
    let (sender, done) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationDone(sender).into();
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let propvariant = activation_propvariant(&mut params);
    // SAFETY: The parameters outlive the call and the API copies the value
    // before returning; the pointer is never used after it.
    let activation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&propvariant),
            &handler,
        )
    };
    activation.map_err(|error| hresult("ActivateAudioInterfaceAsync", &error))?;
    done.recv_timeout(ACTIVATION_TIMEOUT)
        .map_err(|_| "the app's loopback activation did not complete in time".to_owned())?
}

/// The VT_BLOB PROPVARIANT naming the activation parameters, which
/// ActivateAudioInterfaceAsync copies before returning.
fn activation_propvariant(params: &mut AUDIOCLIENT_ACTIVATION_PARAMS) -> PROPVARIANT {
    let blob = BLOB {
        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        pBlobData: std::ptr::from_mut(params).cast::<u8>(),
    };
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { blob },
            }),
        },
    }
}

/// Process loopback's activation completion, which the API calls from its
/// own thread; it is agile because it owns nothing but a channel.
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct ActivationDone(mpsc::Sender<std::result::Result<IAudioClient, String>>);

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationDone_Impl {
    fn ActivateCompleted(
        &self,
        operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let result = operation
            .ok()
            .and_then(|operation| {
                let mut status = HRESULT::default();
                let mut interface: Option<IUnknown> = None;
                // SAFETY: The operation fills the status and the interface it
                // activated; both are this call's own locals.
                unsafe { operation.GetActivateResult(&mut status, &mut interface) }?;
                status.ok()?;
                interface
                    .ok_or_else(|| windows::core::Error::from(E_FAIL))?
                    .cast::<IAudioClient>()
            })
            .map_err(|error| hresult("GetActivateResult", &error));
        let _ = self.0.send(result);
        Ok(())
    }
}

impl IAgileObject_Impl for ActivationDone_Impl {}

/// The capture event SetEventHandle signals, closed on drop.
struct OwnedEvent(HANDLE);

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        // SAFETY: The handle is this struct's own, and closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// The process-tree roots of the app now: its processes whose parent is not
/// one of its own, so a helper and its children are captured once.
fn root_processes(id: &str) -> Option<Vec<u32>> {
    let processes = crate::microphone_activity::running_processes()?;
    let names = processes
        .iter()
        .map(|(pid, parent, name)| (*pid, *parent, name.as_str()))
        .collect::<Vec<_>>();
    Some(root_pids(id, &names))
}

/// The tree roots among (pid, parent pid, executable) whose executable is the
/// id and whose parent's executable is not, sorted: one loopback of a root
/// already covers its children.
fn root_pids(id: &str, processes: &[(u32, u32, &str)]) -> Vec<u32> {
    let is_id = |pid: u32| {
        processes
            .iter()
            .any(|(other, _, name)| *other == pid && name.eq_ignore_ascii_case(id))
    };
    let mut roots = processes
        .iter()
        .filter(|(pid, parent, name)| {
            name.eq_ignore_ascii_case(id) && *pid != 0 && *parent != *pid && !is_id(*parent)
        })
        .map(|(pid, _, _)| *pid)
        .collect::<Vec<_>>();
    roots.sort_unstable();
    roots.dedup();
    roots
}

fn hresult(step: &str, error: &windows::core::Error) -> String {
    format!(
        "{step} failed: {} ({:#010x})",
        error.message(),
        error.code().0 as u32
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_roots_of_an_app_are_its_processes_whose_parent_is_not_its_own() {
        let processes = [
            (100, 4, "explorer.exe"),
            (200, 4, "ms-teams.exe"),
            (201, 200, "ms-teams.exe"),
            (202, 201, "ms-teams.exe"),
            (300, 4, "ms-teams.exe"),
            (400, 200, "msedgewebview2.exe"),
        ];
        assert_eq!(root_pids("ms-teams.exe", &processes), vec![200, 300]);
    }

    #[test]
    fn the_executable_matches_regardless_of_case_and_only_exactly() {
        let processes = [
            (10, 4, "MS-Teams.EXE"),
            (11, 10, "ms-teams.exe"),
            (12, 4, "ms-teams-helper.exe"),
            (13, 4, "teams.exe"),
        ];
        assert_eq!(root_pids("ms-teams.exe", &processes), vec![10]);
        assert_eq!(root_pids("slack.exe", &processes), Vec::<u32>::new());
    }

    #[test]
    fn an_orphan_whose_parent_has_exited_is_a_root() {
        let processes = [(50, 999, "zoom.exe"), (51, 50, "zoom.exe")];
        assert_eq!(root_pids("zoom.exe", &processes), vec![50]);
    }

    #[test]
    fn process_loopback_asks_for_16_bit_stereo_at_the_requested_rate() {
        // WAVEFORMATEX is packed, so each field is copied out before it is
        // compared; a reference to a packed field is unaligned.
        let format = pcm_format(48_000);
        let tag = format.wFormatTag;
        let channels = format.nChannels;
        let rate = format.nSamplesPerSec;
        let bits = format.wBitsPerSample;
        let block = format.nBlockAlign;
        let bytes = format.nAvgBytesPerSec;
        let extra = format.cbSize;
        assert_eq!(tag, WAVE_FORMAT_PCM as u16);
        assert_eq!(channels, 2);
        assert_eq!(rate, 48_000);
        assert_eq!(bits, 16);
        assert_eq!(block, 4);
        assert_eq!(bytes, 48_000 * 4);
        assert_eq!(extra, 0);
    }

    #[test]
    fn interleaved_16_bit_frames_become_mono_float() {
        // Two stereo frames: (0, i16::MAX) and (-i16::MAX, 0).
        let bytes = [0x00, 0x00, 0xFF, 0x7F, 0x01, 0x80, 0x00, 0x00];
        assert_eq!(mono_i16(&bytes, 2), vec![0.5, -0.5]);
        // Mono audio passes through, and a trailing partial frame is dropped.
        let mono = [0xFF, 0x7F, 0x00, 0x00, 0x01];
        assert_eq!(mono_i16(&mono, 1), vec![1.0, 0.0]);
    }
}
