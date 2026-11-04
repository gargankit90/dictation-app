// Included by `build.rs` as well as compiled into the app, so only outer
// doc comments here.

/// Every command in `commands::builder`, in snake case: the commands the UI
/// may call. `build.rs` generates their `allow-<command>` permissions from
/// this list (the app's ACL manifest) and `capabilities/default.json`
/// grants exactly those, and nothing from `core:` (M1.7, #82 M1). With an
/// app manifest, Tauri checks app commands against the capabilities too,
/// so a command left out here can't be called from any window.
pub const COMMAND_NAMES: &[&str] = &[
    "get_settings",
    "save_settings",
    "set_api_key",
    "has_api_key",
    "test_api_key",
    "get_onboarding",
    "request_permission",
    "set_onboarding_step",
    "finish_onboarding",
    "relaunch_app",
    "list_local_models",
    "download_local_model",
    "cancel_local_model_download",
    "delete_local_model",
    "set_share_usage_events",
    "set_hotkey",
    "set_hotkey_recording",
    "run_checks",
    "test_connection",
    "build_report",
    "save_report",
    "open_issue",
    "take_troubleshoot_request",
    "dictation_progress",
];
