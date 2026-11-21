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
    "set_api_key",
    "test_api_key",
    "connect_openrouter",
    "cancel_connect",
    "saved_key_hint",
    "get_onboarding",
    "accept_terms",
    "request_permission",
    "open_setup",
    "set_onboarding_step",
    "finish_onboarding",
    "relaunch_app",
    "list_local_models",
    "list_cleanup_models",
    "download_local_model",
    "cancel_local_model_download",
    "delete_local_model",
    "set_share_usage_events",
    "set_hotkey",
    "set_hotkey_recording",
    "get_hotkey_notice",
    "set_launch_at_login",
    "run_checks",
    "test_connection",
    "build_report",
    "save_report",
    "open_issue",
    "take_window_request",
    "reveal_log_file",
    "set_transcription",
    "get_engine_status",
    "delete_api_key",
    "open_link",
    "get_app_info",
    "check_for_updates",
    "list_history",
    "copy_history_entry",
    "delete_history_entry",
    "clear_history",
    "set_history_limit",
];

/// The overlay window's commands (U4 round 2): `build.rs` adds their
/// `allow-<command>` permissions to the app's ACL manifest, and only
/// `capabilities/overlay.json` grants them. Not in [`COMMAND_NAMES`], so the
/// settings window can't call them.
pub const OVERLAY_COMMAND_NAMES: &[&str] = &["get_overlay_level"];
