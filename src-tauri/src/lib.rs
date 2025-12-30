mod ai;
mod commands;
mod jobs;
mod models;
mod security;
mod services;

use commands::*;
use services::watcher::create_watcher_handle;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let watcher_handle = create_watcher_handle();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .manage(watcher_handle)
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
            build_folder_context,
            generate_organize_plan,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
