use keyring::Entry;
use once_cell::sync::Lazy;
use std::sync::RwLock;
#[cfg(debug_assertions)]
use std::fs;
#[cfg(debug_assertions)]
use std::path::PathBuf;

const SERVICE_NAME: &str = "com.sentinel.filemanager";

/// Cached API keys to avoid repeated keychain/env lookups
///
/// API keys are looked up once per provider and cached in memory.
/// This avoids the overhead of keychain access (~1-5ms) or env var
/// lookup on every API request.
///
/// Cache is invalidated when store_api_key or delete_api_key is called.
static API_KEY_CACHE: Lazy<RwLock<std::collections::HashMap<String, Option<String>>>> =
    Lazy::new(|| RwLock::new(std::collections::HashMap::new()));

/// Credential manager using macOS Keychain with file fallback for development
pub struct CredentialManager;

impl CredentialManager {
    /// Get the fallback file path for storing credentials (dev mode only)
    #[cfg(debug_assertions)]
    fn get_fallback_path(provider: &str) -> Option<PathBuf> {
        dirs::config_dir().map(|dir| {
            let app_dir = dir.join("sentinel");
            app_dir.join(format!("{}_key", provider))
        })
    }

    /// Store an API key using macOS Keychain (with file fallback in dev mode)
    pub fn store_api_key(provider: &str, api_key: &str) -> Result<(), String> {
        // Invalidate cache for this provider
        if let Ok(mut cache) = API_KEY_CACHE.write() {
            cache.remove(provider);
        }

        // Try keychain first (secure storage)
        match Entry::new(SERVICE_NAME, provider) {
            Ok(entry) => {
                if entry.set_password(api_key).is_ok() {
                    #[cfg(debug_assertions)]
                    eprintln!("[Credentials] Stored API key in keychain for: {}", provider);
                    return Ok(());
                }
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!("[Credentials] Keychain unavailable: {}", _e);
            }
        }

        // Fallback to file storage only in debug/dev mode
        #[cfg(debug_assertions)]
        {
            if let Some(path) = Self::get_fallback_path(provider) {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create config directory: {}", e))?;
                }

                // Use base64 encoding for minimal obfuscation in dev mode
                let encoded = base64_encode(api_key);
                fs::write(&path, encoded)
                    .map_err(|e| format!("Failed to write API key: {}", e))?;

                eprintln!("[Credentials] DEV MODE: Stored API key in file: {:?}", path);
                return Ok(());
            }
        }

        #[cfg(not(debug_assertions))]
        {
            return Err("Secure credential storage (Keychain) unavailable".to_string());
        }

        #[cfg(debug_assertions)]
        Err("Could not determine config directory".to_string())
    }

    /// Get an API key - uses compile-time embedded key for Anthropic, Keychain for others
    ///
    /// Results are cached to avoid repeated keychain/env lookups.
    pub fn get_api_key(provider: &str) -> Result<String, String> {
        // Check cache first (fast path)
        if let Ok(cache) = API_KEY_CACHE.read() {
            if let Some(cached) = cache.get(provider) {
                return cached.clone().ok_or_else(|| "API key not found".to_string());
            }
        }

        // Cache miss - perform lookup
        let result = Self::get_api_key_uncached(provider);

        // Update cache
        if let Ok(mut cache) = API_KEY_CACHE.write() {
            cache.insert(provider.to_string(), result.clone().ok());
        }

        result
    }

    /// Get an API key without caching (internal implementation)
    fn get_api_key_uncached(provider: &str) -> Result<String, String> {
        // For Anthropic/Claude: use the app-provided key embedded at compile time
        if provider == "anthropic" {
            // First check compile-time embedded key (for production builds)
            if let Some(key) = option_env!("CLAUDE_API_KEY") {
                if !key.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("[Credentials] Using compile-time embedded CLAUDE_API_KEY");
                    return Ok(key.to_string());
                }
            }

            // Then check runtime env var (for development)
            if let Ok(key) = std::env::var("CLAUDE_API_KEY") {
                if !key.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("[Credentials] Using runtime CLAUDE_API_KEY env var");
                    return Ok(key);
                }
            }

            return Err("CLAUDE_API_KEY not configured. Please contact support.".to_string());
        }

        // For OpenAI: use the app-provided key embedded at compile time (free tier models)
        if provider == "openai" {
            // First check compile-time embedded key (for production builds)
            if let Some(key) = option_env!("OPENAI_API_KEY") {
                if !key.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("[Credentials] Using compile-time embedded OPENAI_API_KEY");
                    return Ok(key.to_string());
                }
            }

            // Then check runtime env var (for development)
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                if !key.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("[Credentials] Using runtime OPENAI_API_KEY env var");
                    return Ok(key);
                }
            }

            return Err("OPENAI_API_KEY not configured. Please contact support.".to_string());
        }

        // For other providers: use Keychain
        if let Ok(entry) = Entry::new(SERVICE_NAME, provider) {
            if let Ok(password) = entry.get_password() {
                #[cfg(debug_assertions)]
                eprintln!("[Credentials] Retrieved API key from keychain for: {}", provider);
                return Ok(password);
            }
        }

        // Fallback to file storage only in debug/dev mode
        #[cfg(debug_assertions)]
        {
            if let Some(path) = Self::get_fallback_path(provider) {
                if path.exists() {
                    let encoded = fs::read_to_string(&path)
                        .map_err(|e| format!("Failed to read API key: {}", e))?;
                    let decoded = base64_decode(&encoded)?;
                    eprintln!("[Credentials] DEV MODE: Retrieved API key from file: {:?}", path);
                    return Ok(decoded);
                }
            }
        }

        Err("API key not found".to_string())
    }

    /// Delete an API key from Keychain and file storage
    pub fn delete_api_key(provider: &str) -> Result<(), String> {
        // Invalidate cache for this provider
        if let Ok(mut cache) = API_KEY_CACHE.write() {
            cache.remove(provider);
        }

        // Delete from keychain
        if let Ok(entry) = Entry::new(SERVICE_NAME, provider) {
            let _ = entry.delete_credential();
            #[cfg(debug_assertions)]
            eprintln!("[Credentials] Deleted API key from keychain for: {}", provider);
        }

        // Also delete from file storage in dev mode
        #[cfg(debug_assertions)]
        {
            if let Some(path) = Self::get_fallback_path(provider) {
                if path.exists() {
                    fs::remove_file(&path)
                        .map_err(|e| format!("Failed to delete API key file: {}", e))?;
                    eprintln!("[Credentials] DEV MODE: Deleted API key file: {:?}", path);
                }
            }
        }

        Ok(())
    }

    /// Check if an API key is configured
    pub fn has_api_key(provider: &str) -> bool {
        Self::get_api_key(provider).is_ok()
    }

    /// Migrate existing file-based credentials to keychain
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    pub fn migrate_to_keychain(provider: &str) -> Result<bool, String> {
        // Check if file-based credential exists
        if let Some(path) = Self::get_fallback_path(provider) {
            if path.exists() {
                // Read from file
                let encoded = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read API key: {}", e))?;
                let api_key = base64_decode(&encoded)?;

                // Store in keychain
                if let Ok(entry) = Entry::new(SERVICE_NAME, provider) {
                    if entry.set_password(&api_key).is_ok() {
                        // Delete the file after successful migration
                        let _ = fs::remove_file(&path);
                        eprintln!("[Credentials] Migrated {} from file to keychain", provider);
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

// Base64 encoding/decoding for dev mode file storage
#[cfg(debug_assertions)]
fn base64_encode(input: &str) -> String {
    use std::io::Write;
    let mut output = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut output);
        encoder.write_all(input.as_bytes()).unwrap();
    }
    String::from_utf8(output).unwrap()
}

#[cfg(debug_assertions)]
fn base64_decode(input: &str) -> Result<String, String> {
    let bytes = base64_decode_bytes(input.trim())?;
    String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {}", e))
}

#[cfg(debug_assertions)]
const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[cfg(debug_assertions)]
struct Base64Encoder<'a, W: std::io::Write> {
    writer: &'a mut W,
    buffer: [u8; 3],
    buffer_len: usize,
}

#[cfg(debug_assertions)]
impl<'a, W: std::io::Write> Base64Encoder<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            buffer: [0; 3],
            buffer_len: 0,
        }
    }

    fn flush_buffer(&mut self) -> std::io::Result<()> {
        if self.buffer_len == 0 {
            return Ok(());
        }

        let b0 = self.buffer[0] as usize;
        let b1 = if self.buffer_len > 1 { self.buffer[1] as usize } else { 0 };
        let b2 = if self.buffer_len > 2 { self.buffer[2] as usize } else { 0 };

        let c0 = BASE64_CHARS[b0 >> 2];
        let c1 = BASE64_CHARS[((b0 & 0x03) << 4) | (b1 >> 4)];
        let c2 = if self.buffer_len > 1 {
            BASE64_CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)]
        } else {
            b'='
        };
        let c3 = if self.buffer_len > 2 {
            BASE64_CHARS[b2 & 0x3f]
        } else {
            b'='
        };

        self.writer.write_all(&[c0, c1, c2, c3])?;
        self.buffer_len = 0;
        Ok(())
    }
}

#[cfg(debug_assertions)]
impl<'a, W: std::io::Write> std::io::Write for Base64Encoder<'a, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            self.buffer[self.buffer_len] = byte;
            self.buffer_len += 1;
            if self.buffer_len == 3 {
                self.flush_buffer()?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_buffer()
    }
}

#[cfg(debug_assertions)]
impl<'a, W: std::io::Write> Drop for Base64Encoder<'a, W> {
    fn drop(&mut self) {
        let _ = self.flush_buffer();
    }
}

#[cfg(debug_assertions)]
fn base64_decode_bytes(input: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let input = input.as_bytes();

    let decode_char = |c: u8| -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err(format!("Invalid base64 character: {}", c as char)),
        }
    };

    let mut i = 0;
    while i < input.len() {
        if i + 4 > input.len() {
            break;
        }

        let c0 = decode_char(input[i])?;
        let c1 = decode_char(input[i + 1])?;
        let c2 = decode_char(input[i + 2])?;
        let c3 = decode_char(input[i + 3])?;

        output.push((c0 << 2) | (c1 >> 4));
        if input[i + 2] != b'=' {
            output.push((c1 << 4) | (c2 >> 2));
        }
        if input[i + 3] != b'=' {
            output.push((c2 << 6) | c3);
        }

        i += 4;
    }

    Ok(output)
}
