mod ai;
mod billing;
mod commands;
mod execution;
mod file_coordination;
mod history;
mod jobs;
mod models;
pub mod quarantine;
mod security;
mod services;
mod tree;
pub mod utils;
mod vector;
pub mod vfs;
mod wal;

use billing::BillingState;
use commands::*;
use commands::grok::{GrokState, GrokAbortFlag};
use services::watcher::create_watcher_handle;
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env file - try multiple locations
    // During `tauri dev`, CWD is project root; check current dir first
    if dotenvy::dotenv().is_err() {
        // Fallback: check parent directory (if running from src-tauri)
        let _ = dotenvy::from_path("../.env");
    }

    // Initialize tracing with RUST_LOG env filter
    // Default: warn for most crates, info for our app (job summaries visible)
    // Use RUST_LOG=debug for verbose per-operation logs
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,tauri_app_lib=info")),
        )
        .init();

    let watcher_handle = create_watcher_handle();
    let vector_state = VectorState::default();
    let tree_state = TreeState::default();
    let vfs_state = create_vfs_state();
    let quarantine_state = create_quarantine_state()
        .unwrap_or_else(|e| {
            tracing::error!("Failed to create quarantine manager: {}. Using fallback.", e);
            // Create a fallback quarantine manager with temp directory
            use std::sync::{Arc, RwLock};
            use quarantine::QuarantineManager;
            Arc::new(RwLock::new(QuarantineManager::with_config(
                std::env::temp_dir().join("sentinel-quarantine"),
                30
            )))
        });
    let chat_abort_flag = ChatAbortFlag::default();
    let grok_state = GrokState::default();
    let grok_abort_flag = GrokAbortFlag::default();
    let billing_state = BillingState::default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage(watcher_handle)
        .manage(vector_state)
        .manage(tree_state)
        .manage(vfs_state)
        .manage(quarantine_state)
        .manage(chat_abort_flag)
        .manage(grok_state)
        .manage(grok_abort_flag)
        .manage(billing_state)
        .invoke_handler(tauri::generate_handler![
            // Filesystem commands
            read_directory,
            get_file_metadata,
            rename_file,
            delete_to_trash,
            create_directory,
            create_file,
            move_file,
            copy_file,
            get_home_directory,
            get_downloads_directory,
            get_user_directories,
            open_file,
            // Drag-drop commands
            validate_drag_drop,
            move_files_batch,
            copy_files_batch,
            // Watcher commands
            start_downloads_watcher,
            stop_downloads_watcher,
            get_watcher_status,
            add_watched_folder,
            remove_watched_folder,
            // AI commands
            set_api_key,
            delete_api_key,
            get_configured_providers,
            get_rename_suggestion,
            apply_rename,
            undo_rename,
            get_batch_rename_suggestions,
            apply_batch_rename,
            generate_organize_plan_hybrid,
            generate_simplification_plan,
            // Job persistence commands
            start_organize_job,
            set_job_plan,
            complete_job_operation,
            complete_organize_job,
            fail_organize_job,
            check_interrupted_job,
            get_current_job,
            clear_organize_job,
            resume_organize_job,
            execute_plan_parallel,
            capture_state_snapshot,
            validate_state_snapshot,
            // Thumbnail commands
            get_thumbnail,
            clear_thumbnail_cache,
            get_thumbnail_cache_stats,
            // Permission commands
            check_path_permission,
            get_protected_directories,
            open_privacy_settings,
            // Shell permission commands
            get_shell_permissions,
            allow_shell_command,
            revoke_shell_command,
            check_shell_command,
            // Photo commands
            scan_photos,
            get_photo_directories,
            // Vector index commands
            init_vector_index,
            vector_search,
            vector_get_tags,
            vector_find_by_tag,
            vector_find_similar,
            vector_all_tags,
            vector_stats,
            clear_vector_index,
            // Tree compression commands
            get_tree_xml,
            configure_tree,
            get_tree_config,
            // VFS commands
            scan_folder_vfs,
            vfs_list_dir,
            vfs_search_content,
            vfs_get_node,
            vfs_get_stats,
            vfs_validate_plan,
            vfs_simulate_plan,
            vfs_stage_move,
            vfs_stage_create_folder,
            vfs_stage_delete,
            vfs_apply_staged,
            vfs_clear_staged,
            vfs_has_staged,
            vfs_clear,
            // Quarantine commands
            quarantine_item,
            quarantine_restore,
            quarantine_list,
            quarantine_cleanup,
            quarantine_permanent_delete,
            quarantine_check,
            // WAL commands
            wal_check_recovery,
            wal_resume_job,
            wal_rollback_job,
            wal_discard_job,
            wal_get_journal,
            wal_list_journals,
            wal_create_journal,
            wal_add_operation,
            wal_execute_journal,
            wal_execute_operations,
            wal_get_directory,
            // Chat commands
            chat_stream,
            abort_chat,
            reset_chat_abort,
            list_files_for_mention,
            // Grok AI commands
            grok_init,
            grok_scan_folder,
            grok_organize,
            grok_generate_plan,
            grok_execute_plan,
            grok_analyze_file,
            grok_cache_stats,
            grok_clear_cache,
            grok_check_api_key,
            grok_set_api_key,
            grok_get_api_key,
            grok_abort_plan,
            grok_reset_abort,
            // Billing commands
            get_daily_usage,
            get_usage_history,
            check_request_limit,
            update_subscription_cache,
            get_subscription,
            get_subscription_info,
            clear_subscription_cache,
            record_usage,
            can_use_model,
            can_use_extended_thinking,
            check_token_quota,
            get_monthly_tokens,
            // History commands (multi-level undo)
            history_has_history,
            history_get_summary,
            history_get_sessions,
            history_get_session_detail,
            history_undo_preflight,
            history_undo_execute,
            history_delete,
            history_list_folders,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
