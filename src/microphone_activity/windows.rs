//! Microphone users on Windows: the processes with an active audio session
//! on any capture device, from `IAudioSessionManager2`, and the apps still
//! playing through an active render session. Only session metadata is read;
//! no audio is captured.

use color_eyre::eyre::{Result, eyre};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, DEVICE_STATE_ACTIVE, EDataFlow, IAudioSessionControl2,
    IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::core::Interface;

use super::{ActiveMicrophoneApplication, canonical_bundle_id_for, deduplicated};

/// One active endpoint and the pids with an active session on it.
pub(crate) struct EndpointSessions {
    pub(crate) endpoint_id: String,
    pub(crate) pids: Vec<u32>,
}

/// Every active endpoint of `flow` and the pids with an active
/// (`AudioSessionStateActive`) session on it, its own pid excluded.
pub(crate) fn active_endpoint_sessions(flow: EDataFlow) -> Result<Vec<EndpointSessions>> {
    // SAFETY: Joins this thread to the multithreaded apartment. A thread
    // already in it gets `S_FALSE` and a thread already in a single-threaded
    // apartment gets `RPC_E_CHANGED_MODE`; both are fine, because the calls
    // below need no message pump and do not depend on the apartment.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let own_pid = std::process::id();
    let endpoints = (|| -> windows::core::Result<Vec<EndpointSessions>> {
        // SAFETY: Plain COM calls on interfaces this function owns. `GetId`
        // hands back a `CoTaskMemAlloc` string that is read once into a
        // `String` and freed once with `CoTaskMemFree`, never used after.
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let devices = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
            let mut endpoints = Vec::new();
            for device in 0..devices.GetCount()? {
                let Ok(device) = devices.Item(device) else {
                    continue;
                };
                let Ok(id) = device.GetId() else {
                    continue;
                };
                let endpoint_id = id.to_string();
                CoTaskMemFree(Some(id.as_ptr().cast_const().cast()));
                let Ok(endpoint_id) = endpoint_id else {
                    continue;
                };
                let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                    continue;
                };
                let Ok(sessions) = manager.GetSessionEnumerator() else {
                    continue;
                };
                let mut pids = Vec::new();
                for session in 0..sessions.GetCount()? {
                    let Ok(control) = sessions.GetSession(session) else {
                        continue;
                    };
                    if control.GetState().ok() != Some(AudioSessionStateActive) {
                        continue;
                    }
                    if let Ok(control) = control.cast::<IAudioSessionControl2>()
                        && let Ok(pid) = control.GetProcessId()
                        && pid != 0
                        && pid != own_pid
                    {
                        pids.push(pid);
                    }
                }
                endpoints.push(EndpointSessions { endpoint_id, pids });
            }
            Ok(endpoints)
        }
    })()
    .map_err(|error| eyre!("could not read audio sessions: {error}"))?;
    Ok(endpoints)
}

/// The processes capturing from a microphone now.
pub fn active_microphone_applications() -> Result<Vec<ActiveMicrophoneApplication>> {
    let mut applications = Vec::new();
    for endpoint in active_endpoint_sessions(eCapture)? {
        for pid in endpoint.pids {
            let Some(path) = process_path(pid) else {
                continue;
            };
            let Some(executable) = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            let name = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| executable.clone());
            applications.push(ActiveMicrophoneApplication {
                pid: pid as i32,
                bundle_id: executable,
                name,
            });
        }
    }
    Ok(deduplicated(applications))
}

/// Whether any process of the app `canonical_bundle_id` (its helpers
/// included) has an active render session: a call keeps its output open,
/// muted or not, and a hung-up call closes it with the microphone (#835).
pub(crate) fn application_running_output(canonical_bundle_id: &str) -> Result<bool> {
    Ok(active_endpoint_sessions(eRender)?
        .iter()
        .flat_map(|endpoint| endpoint.pids.iter().copied())
        .any(|pid| {
            process_canonical_bundle_id(pid)
                .is_some_and(|canonical| canonical == canonical_bundle_id)
        }))
}

/// The canonical bundle identifier of `pid`'s executable, when Hex knows it.
pub(crate) fn process_canonical_bundle_id(pid: u32) -> Option<&'static str> {
    let path = process_path(pid)?;
    let executable = path.file_name()?.to_string_lossy().into_owned();
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned());
    canonical_bundle_id_for(&executable, name.as_deref())
}

/// Every running process, from one Toolhelp snapshot: its pid, its parent's
/// pid, and its executable's lowercased file name. `None` when the list
/// cannot be read.
pub(crate) fn running_processes() -> Option<Vec<(u32, u32, String)>> {
    use windows::Win32::Foundation::CloseHandle;
    // SAFETY: The snapshot handle is checked, walked once, and closed; each
    // entry is a PROCESSENTRY32W this call owns.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_err() {
            let _ = CloseHandle(snapshot);
            return None;
        }
        let mut processes = Vec::new();
        loop {
            processes.push((
                entry.th32ProcessID,
                entry.th32ParentProcessID,
                entry_name(&entry),
            ));
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
        Some(processes)
    }
}

/// A PROCESSENTRY32W's executable file name, lowercased to compare with an
/// app id.
fn entry_name(entry: &PROCESSENTRY32W) -> String {
    let length = entry
        .szExeFile
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..length]).to_ascii_lowercase()
}

/// The processes an audio session is never attributed past: the desktop, the
/// service host, the service control manager, and the session manager. An
/// app's helper walks up to its app, and never to one of these.
const SYSTEM_EXECUTABLES: [&str; 4] =
    ["explorer.exe", "svchost.exe", "services.exe", "wininit.exe"];

/// The process whose executable names the app playing `pid`'s audio: the
/// top-most ancestor that is not a shell or service process. A WebView2
/// helper such as the new Teams' `msedgewebview2.exe` children is Teams, so
/// the pane lists Teams once and the capture opens Teams' whole tree. The
/// walk stops at a shell or system process, at a reserved pid (0 to 4, the
/// System and Idle processes), at a parent that has exited, and at a cycle,
/// so it always ends; a pid the list does not hold answers itself.
pub(crate) fn application_pid(pid: u32, processes: &[(u32, u32, &str)]) -> u32 {
    let parent_of = |pid: u32| {
        processes
            .iter()
            .find(|(other, _, _)| *other == pid)
            .map(|(_, parent, _)| *parent)
    };
    let mut app = pid;
    let mut seen = vec![pid];
    while let Some(parent) = parent_of(app) {
        if parent <= 4 || seen.contains(&parent) {
            break;
        }
        let Some((_, _, name)) = processes.iter().find(|(other, _, _)| *other == parent) else {
            break;
        };
        if SYSTEM_EXECUTABLES
            .iter()
            .any(|system| name.eq_ignore_ascii_case(system))
        {
            break;
        }
        seen.push(parent);
        app = parent;
    }
    app
}

/// `pid`'s executable file name, lowercased (such as `ms-teams.exe`), and
/// its file stem as a display name, for Labs › Choose meeting audio.
pub(crate) fn process_application(pid: u32) -> Option<(String, String)> {
    let path = process_path(pid)?;
    let executable = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable.clone());
    Some((executable, name))
}

fn process_path(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    // SAFETY: The handle is checked, used once, and closed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = [0_u16; 1024];
        let mut length = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call's WebView2 children are the app that plays its audio, so the
    /// pane lists Teams rather than every WebView2 tree on the machine, and
    /// the walk never climbs into the shell or the services.
    #[test]
    fn an_audio_session_belongs_to_its_top_most_ancestor_outside_the_shell() {
        let processes = [
            (4, 0, "system"),
            (200, 4, "services.exe"),
            (300, 4, "explorer.exe"),
            (400, 300, "ms-teams.exe"),
            (401, 400, "msedgewebview2.exe"),
            (402, 401, "msedgewebview2.exe"),
            (500, 300, "chrome.exe"),
            (501, 500, "chrome.exe"),
            // A service plays through its own helper: the walk stops at the
            // service host.
            (600, 4, "svchost.exe"),
            (601, 600, "audiodg.exe"),
            // The parent already exited, and a pid the list does not hold.
            (800, 999, "zoom.exe"),
        ];
        assert_eq!(application_pid(401, &processes), 400);
        assert_eq!(application_pid(402, &processes), 400);
        assert_eq!(application_pid(400, &processes), 400);
        assert_eq!(application_pid(501, &processes), 500);
        assert_eq!(application_pid(601, &processes), 600);
        assert_eq!(application_pid(800, &processes), 800);
        assert_eq!(application_pid(900, &processes), 900);
        // Explorer itself stays the app it is, rather than climbing to
        // services.
        assert_eq!(application_pid(300, &processes), 300);
    }

    /// A parent chain that loops back on itself ends at the last app before
    /// the loop, so the walk never hangs.
    #[test]
    fn a_parent_cycle_stops_the_ancestor_walk() {
        let processes = [(10, 11, "app.exe"), (11, 10, "other.exe")];
        assert_eq!(application_pid(10, &processes), 11);
        assert_eq!(application_pid(11, &processes), 10);
    }

    /// A name from the snapshot is compared without regard to the case the
    /// executable was saved as.
    #[test]
    fn an_app_id_matches_its_executable_regardless_of_case() {
        let processes = [(100, 4, "MS-Teams.EXE"), (101, 100, "msedgewebview2.exe")];
        assert_eq!(application_pid(101, &processes), 100);
        assert_eq!(
            processes[0].2.to_ascii_lowercase(),
            "ms-teams.exe",
            "the listing lowercases it as the saved app id"
        );
    }
}
