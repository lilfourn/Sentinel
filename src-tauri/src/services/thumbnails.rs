use base64::{engine::general_purpose::STANDARD, Engine};
use image::{imageops::FilterType, ImageFormat, RgbaImage};
use resvg::tiny_skia::Pixmap;
use resvg::usvg::{Options, Tree};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const DEFAULT_THUMBNAIL_SIZE: u32 = 96;

/// Get the cache directory for thumbnails
fn get_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|p| p.join("com.sentinel.app").join("thumbnails"))
}

/// Generate a cache key from file path and size
fn get_cache_key(file_path: &str, size: u32, mtime: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    let hash = hasher.finalize();
    let hash_hex: String = hash.iter().take(8).map(|b| format!("{:02x}", b)).collect();
    format!("{}_{}_{}.webp", hash_hex, size, mtime)
}

/// Get the modification time of a file as unix timestamp
fn get_file_mtime(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Check if cached thumbnail exists and is valid
fn get_cached_thumbnail(file_path: &str, size: u32) -> Option<String> {
    let cache_dir = get_cache_dir()?;
    let mtime = get_file_mtime(Path::new(file_path))?;
    let cache_key = get_cache_key(file_path, size, mtime);
    let cache_path = cache_dir.join(&cache_key);

    if cache_path.exists() {
        fs::read(&cache_path)
            .ok()
            .map(|bytes| STANDARD.encode(&bytes))
    } else {
        None
    }
}

/// Save thumbnail to cache
fn save_to_cache(file_path: &str, size: u32, data: &[u8]) -> Result<(), String> {
    let cache_dir = get_cache_dir().ok_or("Failed to get cache directory")?;
    fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let mtime = get_file_mtime(Path::new(file_path)).unwrap_or(0);
    let cache_key = get_cache_key(file_path, size, mtime);
    let cache_path = cache_dir.join(&cache_key);

    fs::write(&cache_path, data).map_err(|e| format!("Failed to write cache: {}", e))
}

/// Generate thumbnail for an image file
fn generate_image_thumbnail(path: &Path, size: u32) -> Result<Vec<u8>, String> {
    let img = image::open(path).map_err(|e| format!("Failed to open image: {}", e))?;

    // Resize maintaining aspect ratio
    let thumbnail = img.resize(size, size, FilterType::Lanczos3);

    // Encode as WebP (or PNG as fallback since webp encoding might not be available)
    let mut buffer = Cursor::new(Vec::new());

    // Try WebP first, fallback to PNG
    if thumbnail
        .write_to(&mut buffer, ImageFormat::WebP)
        .is_err()
    {
        buffer = Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;
    }

    Ok(buffer.into_inner())
}

/// Generate thumbnail for a video file using ffmpeg
fn generate_video_thumbnail(path: &Path, size: u32) -> Result<Vec<u8>, String> {
    // Check if ffmpeg is available
    let ffmpeg_check = Command::new("ffmpeg").arg("-version").output();

    if ffmpeg_check.is_err() {
        return Err("ffmpeg not installed".to_string());
    }

    let temp_dir = std::env::temp_dir();
    let temp_output = temp_dir.join(format!("sentinel_thumb_{}.png", std::process::id()));

    // Extract first frame using ffmpeg
    let output = Command::new("ffmpeg")
        .args([
            "-i",
            path.to_str().ok_or("Invalid path")?,
            "-vf",
            &format!("scale={}:{}:force_original_aspect_ratio=decrease", size, size),
            "-vframes",
            "1",
            "-y",
            temp_output.to_str().ok_or("Invalid temp path")?,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        let _ = fs::remove_file(&temp_output);
        return Err(format!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Read the generated thumbnail
    let data = fs::read(&temp_output).map_err(|e| format!("Failed to read thumbnail: {}", e))?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_output);

    // Convert to WebP/PNG via image crate for consistency
    let img = image::load_from_memory(&data).map_err(|e| format!("Failed to load frame: {}", e))?;
    let mut buffer = Cursor::new(Vec::new());
    img.write_to(&mut buffer, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode: {}", e))?;

    Ok(buffer.into_inner())
}

/// Generate thumbnail for a PDF file using ffmpeg (it supports PDF)
fn generate_pdf_thumbnail(path: &Path, size: u32) -> Result<Vec<u8>, String> {
    // ffmpeg can extract first page from PDF
    // Alternatively, we could use a dedicated PDF library
    generate_video_thumbnail(path, size)
}

/// Generate thumbnail for an SVG file using resvg
fn generate_svg_thumbnail(path: &Path, size: u32) -> Result<Vec<u8>, String> {
    // Read SVG content
    let svg_data = fs::read(path).map_err(|e| format!("Failed to read SVG: {}", e))?;

    // Parse SVG
    let options = Options::default();
    let tree =
        Tree::from_data(&svg_data, &options).map_err(|e| format!("Failed to parse SVG: {}", e))?;

    // Get original size
    let svg_size = tree.size();
    let (orig_width, orig_height) = (svg_size.width(), svg_size.height());

    // Calculate scaled size maintaining aspect ratio
    let scale = if orig_width > orig_height {
        size as f32 / orig_width
    } else {
        size as f32 / orig_height
    };

    let width = (orig_width * scale).ceil() as u32;
    let height = (orig_height * scale).ceil() as u32;

    // Create pixmap for rendering
    let mut pixmap = Pixmap::new(width, height).ok_or("Failed to create pixmap")?;

    // Render SVG
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Convert to image crate format for consistent output
    let img = RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .ok_or("Failed to create image from pixmap")?;

    // Encode as PNG
    let mut buffer = Cursor::new(Vec::new());
    img.write_to(&mut buffer, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode SVG thumbnail: {}", e))?;

    Ok(buffer.into_inner())
}

/// Main function to generate or retrieve a thumbnail
pub fn get_thumbnail(file_path: &str, size: Option<u32>) -> Result<String, String> {
    let size = size.unwrap_or(DEFAULT_THUMBNAIL_SIZE);
    let path = Path::new(file_path);

    // Validate file exists
    if !path.exists() {
        return Err("File not found".to_string());
    }

    // Check cache first
    if let Some(cached) = get_cached_thumbnail(file_path, size) {
        return Ok(cached);
    }

    // Get extension
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // Generate based on file type
    let thumbnail_data = match extension.as_str() {
        // Images
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "ico" | "tiff" | "tif" => {
            generate_image_thumbnail(path, size)?
        }
        // SVG (vector graphics)
        "svg" => generate_svg_thumbnail(path, size)?,
        // Videos
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "wmv" | "flv" => {
            generate_video_thumbnail(path, size)?
        }
        // PDF
        "pdf" => generate_pdf_thumbnail(path, size)?,
        _ => return Err("Unsupported file type".to_string()),
    };

    // Save to cache
    let _ = save_to_cache(file_path, size, &thumbnail_data);

    // Return as base64
    Ok(STANDARD.encode(&thumbnail_data))
}

/// Clear the thumbnail cache
pub fn clear_cache() -> Result<u64, String> {
    let cache_dir = get_cache_dir().ok_or("Failed to get cache directory")?;

    if !cache_dir.exists() {
        return Ok(0);
    }

    let mut count = 0u64;
    for entry in fs::read_dir(&cache_dir).map_err(|e| format!("Failed to read cache dir: {}", e))?.flatten() {
        if entry.path().extension().map(|e| e == "webp" || e == "png").unwrap_or(false)
            && fs::remove_file(entry.path()).is_ok()
        {
            count += 1;
        }
    }

    Ok(count)
}

/// Get cache statistics
pub fn get_cache_stats() -> Result<CacheStats, String> {
    let cache_dir = get_cache_dir().ok_or("Failed to get cache directory")?;

    if !cache_dir.exists() {
        return Ok(CacheStats {
            file_count: 0,
            total_size_bytes: 0,
            cache_path: cache_dir.to_string_lossy().to_string(),
        });
    }

    let mut file_count = 0u64;
    let mut total_size = 0u64;

    for entry in fs::read_dir(&cache_dir).map_err(|e| format!("Failed to read cache dir: {}", e))?.flatten() {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                file_count += 1;
                total_size += metadata.len();
            }
        }
    }

    Ok(CacheStats {
        file_count,
        total_size_bytes: total_size,
        cache_path: cache_dir.to_string_lossy().to_string(),
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub file_count: u64,
    pub total_size_bytes: u64,
    pub cache_path: String,
}
