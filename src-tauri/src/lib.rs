mod ai;
mod commands;
mod execution;
mod file_coordination;
mod jobs;
mod models;
pub mod quarantine;
mod security;
mod services;
mod tree;
mod vector;
pub mod vfs;
mod wal;

use commands::*;
use services::watcher::create_watcher_handle;
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        .expect("Failed to create quarantine manager");
    let chat_abort_flag = ChatAbortFlag::default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .manage(watcher_handle)
        .manage(vector_state)
        .manage(tree_state)
        .manage(vfs_state)
        .manage(quarantine_state)
        .manage(chat_abort_flag)
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
            // AI commands
            set_api_key,
            delete_api_key,
            get_configured_providers,
            get_rename_suggestion,
            apply_rename,
            undo_rename,
            generate_organize_plan_agentic,
            suggest_naming_conventions,
            generate_organize_plan_with_convention,
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
            // Thumbnail commands
            get_thumbnail,
            clear_thumbnail_cache,
            get_thumbnail_cache_stats,
            // Permission commands
            check_path_permission,
            get_protected_directories,
            open_privacy_settings,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
