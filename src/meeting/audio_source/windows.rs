//! Windows' meeting audio sources: the active render sessions' executables
//! and the listed output endpoints.

use windows::Win32::Media::Audio::eRender;

use super::SourceEntry;

/// The applications with an active render session now. The id is the
/// lowercase executable file name, such as `ms-teams.exe`, which a call
/// keeps across updates of its display name.
pub(super) fn playing_apps() -> Vec<SourceEntry> {
    let Ok(endpoints) = crate::microphone_activity::active_endpoint_sessions(eRender) else {
        return Vec::new();
    };
    let mut apps: Vec<SourceEntry> = Vec::new();
    for pid in endpoints
        .iter()
        .flat_map(|endpoint| endpoint.pids.iter().copied())
    {
        let Some((id, name)) = crate::microphone_activity::process_application(pid) else {
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
