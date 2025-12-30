use std::path::Path;

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tiff", "tif", "raw", "cr2", "nef",
    "arw", "svg", "ico",
];

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoScanResult {
    pub photos: Vec<PhotoEntry>,
    pub total_count: usize,
    pub scan_duration_ms: u64,
    pub directories_scanned: usize,
}

/// Scan multiple directories for photos
#[tauri::command]
pub async fn scan_photos(directories: Vec<String>) -> Result<PhotoScanResult, String> {
    let start = std::time::Instant::now();
    let mut photos = Vec::new();
    let mut dirs_scanned = 0;

    for dir_path in &directories {
        let path = Path::new(dir_path);
        if path.exists() && path.is_dir() {
            if let Ok(entries) = scan_directory_for_photos(path, 3) {
                photos.extend(entries);
                dirs_scanned += 1;
            }
        }
    }

    // Sort by date (newest first)
    photos.sort_by(|a, b| {
        let a_date = a.created_at.or(a.modified_at).unwrap_or(0);
        let b_date = b.created_at.or(b.modified_at).unwrap_or(0);
        b_date.cmp(&a_date)
    });

    let total_count = photos.len();

    Ok(PhotoScanResult {
        photos,
        total_count,
        scan_duration_ms: start.elapsed().as_millis() as u64,
        directories_scanned: dirs_scanned,
    })
}

/// Get the default directories to scan for photos
#[tauri::command]
pub fn get_photo_directories() -> Vec<(String, String)> {
    let mut dirs = Vec::new();

    if let Some(pictures) = dirs::picture_dir() {
        dirs.push(("Pictures".to_string(), pictures.to_string_lossy().to_string()));
    }
    if let Some(desktop) = dirs::desktop_dir() {
        dirs.push(("Desktop".to_string(), desktop.to_string_lossy().to_string()));
    }
    if let Some(downloads) = dirs::download_dir() {
        dirs.push(("Downloads".to_string(), downloads.to_string_lossy().to_string()));
    }
    if let Some(documents) = dirs::document_dir() {
        dirs.push(("Documents".to_string(), documents.to_string_lossy().to_string()));
    }

    dirs
}

/// Recursively scan a directory for photos
fn scan_directory_for_photos(dir: &Path, max_depth: usize) -> std::io::Result<Vec<PhotoEntry>> {
    let mut photos = Vec::new();

    if max_depth == 0 {
        return Ok(photos);
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            // Skip directories we can't read (permission denied, etc.)
            eprintln!("Cannot read directory {:?}: {}", dir, e);
            return Ok(photos);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories
        if path
            .file_name()
            .map(|n| n.to_string_lossy().starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        if path.is_dir() {
            // Recursively scan subdirectories
            if let Ok(sub_photos) = scan_directory_for_photos(&path, max_depth - 1) {
                photos.extend(sub_photos);
            }
        } else if path.is_file() {
            // Check if it's an image file
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if IMAGE_EXTENSIONS.contains(&ext_lower.as_str()) {
                    if let Ok(photo) = create_photo_entry(&path) {
                        photos.push(photo);
                    }
                }
            }
        }
    }

    Ok(photos)
}

/// Create a PhotoEntry from a file path
fn create_photo_entry(path: &Path) -> std::io::Result<PhotoEntry> {
    let metadata = std::fs::metadata(path)?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let extension = path.extension().map(|e| e.to_string_lossy().to_string());

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    let created_at = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    Ok(PhotoEntry {
        path: path.to_string_lossy().to_string(),
        name,
        size: metadata.len(),
        created_at,
        modified_at,
        extension,
    })
}
