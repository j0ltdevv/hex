//! Windows' meeting audio sources: the active render sessions' apps and the
//! listed output endpoints.

use windows::Win32::Media::Audio::eRender;

use super::SourceEntry;

/// The applications with an active render session now, each named by the
/// top-most process of its tree that is not a shell or service process: the
/// new Teams plays a call's audio from `msedgewebview2.exe` children, which
/// are listed as `ms-teams.exe` and captured with its whole tree. The id is
/// that app's lowercase executable file name, which a call keeps across
/// updates of its display name.
pub(super) fn playing_apps() -> Vec<SourceEntry> {
    let Ok(endpoints) = crate::microphone_activity::active_endpoint_sessions(eRender) else {
        return Vec::new();
    };
    // Without a snapshot each session pid answers for itself, as before.
    let processes = crate::microphone_activity::running_processes().unwrap_or_default();
    let listed = processes
        .iter()
        .map(|(pid, parent, name)| (*pid, *parent, name.as_str()))
        .collect::<Vec<_>>();
    let mut apps: Vec<SourceEntry> = Vec::new();
    for pid in endpoints
        .iter()
        .flat_map(|endpoint| endpoint.pids.iter().copied())
    {
        let app = crate::microphone_activity::application_pid(pid, &listed);
        let Some((id, name)) = crate::microphone_activity::process_application(app) else {
            continue;
        };
        if !apps.iter().any(|app| app.id == id) {
            apps.push(SourceEntry { id, name });
        }
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// The listed output endpoints, the default first, named exactly as the
/// capture source matches a saved output choice.
pub(super) fn output_devices() -> Vec<SourceEntry> {
    crate::meeting::os::output_choices()
        .into_iter()
        .map(|(id, name)| SourceEntry { id, name })
        .collect()
}
