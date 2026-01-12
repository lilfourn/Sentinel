//! SQLite-backed usage tracking
//!
//! Tracks daily API usage per model with atomic increments.
//! Data persists locally and can be synced with Convex.
//!
//! Note: Daily boundaries are based on user's local time, not UTC.
//! This provides a more intuitive experience where limits reset at local midnight.

use chrono::{Local, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tracing::{debug, warn};

use super::types::DailyUsage;

/// Valid model column names for SQL queries - prevents injection
const VALID_MODEL_COLUMNS: &[&str] = &[
    "haiku_requests",
    "sonnet_requests",
    "gpt52_requests",
    "gpt5mini_requests",
    "gpt5nano_requests",
];

/// Helper to acquire mutex lock with poison recovery
/// Instead of panicking on poisoned mutex, recovers the inner value
fn acquire_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned: PoisonError<MutexGuard<'_, T>>| {
        warn!("Mutex was poisoned, recovering inner value");
        poisoned.into_inner()
    })
}

/// SQLite-backed usage tracker
pub struct UsageTracker {
    conn: Mutex<Connection>,
}

impl UsageTracker {
    /// Create or open usage database at ~/.config/sentinel/usage.db
    pub fn new() -> Result<Self, String> {
        let db_path = Self::get_db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open usage database: {}", e))?;

        // Create tables
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS daily_usage (
                user_id TEXT NOT NULL,
                date TEXT NOT NULL,
                haiku_requests INTEGER DEFAULT 0,
                sonnet_requests INTEGER DEFAULT 0,
                opus_requests INTEGER DEFAULT 0,
                extended_thinking_requests INTEGER DEFAULT 0,
                total_input_tokens INTEGER DEFAULT 0,
                total_output_tokens INTEGER DEFAULT 0,
                organize_requests INTEGER DEFAULT 0,
                rename_requests INTEGER DEFAULT 0,
                PRIMARY KEY (user_id, date)
            );

            CREATE INDEX IF NOT EXISTS idx_usage_date
                ON daily_usage(user_id, date DESC);
        "#,
        )
        .map_err(|e| format!("Failed to create tables: {}", e))?;

        // Migration: Add organize_requests and rename_requests columns if they don't exist
        let _ = conn.execute(
            "ALTER TABLE daily_usage ADD COLUMN organize_requests INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE daily_usage ADD COLUMN rename_requests INTEGER DEFAULT 0",
            [],
        );

        // Migration: Add GPT model columns if they don't exist
        let _ = conn.execute(
            "ALTER TABLE daily_usage ADD COLUMN gpt52_requests INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE daily_usage ADD COLUMN gpt5mini_requests INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE daily_usage ADD COLUMN gpt5nano_requests INTEGER DEFAULT 0",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn get_db_path() -> Result<PathBuf, String> {
        dirs::config_dir()
            .map(|d| d.join("sentinel").join("usage.db"))
            .ok_or_else(|| "Could not determine config directory".to_string())
    }

    /// Get today's date string in user's local timezone
    /// This ensures daily limits reset at local midnight, not UTC midnight
    fn today_local() -> String {
        Local::now().format("%Y-%m-%d").to_string()
    }

    /// Get usage for today
    pub fn get_today_usage(&self, user_id: &str) -> Result<DailyUsage, String> {
        let conn = acquire_lock(&self.conn);
        let today = Self::today_local();

        let result = conn.query_row(
            "SELECT date, haiku_requests, sonnet_requests, opus_requests,
                    extended_thinking_requests, total_input_tokens, total_output_tokens,
                    COALESCE(organize_requests, 0), COALESCE(rename_requests, 0),
                    COALESCE(gpt52_requests, 0), COALESCE(gpt5mini_requests, 0), COALESCE(gpt5nano_requests, 0)
             FROM daily_usage WHERE user_id = ? AND date = ?",
            params![user_id, today],
            |row| {
                Ok(DailyUsage {
                    date: row.get(0)?,
                    haiku_requests: row.get(1)?,
                    sonnet_requests: row.get(2)?,
                    opus_requests: row.get(3)?,
                    extended_thinking_requests: row.get(4)?,
                    total_input_tokens: row.get(5)?,
                    total_output_tokens: row.get(6)?,
                    organize_requests: row.get(7)?,
                    rename_requests: row.get(8)?,
                    gpt52_requests: row.get(9)?,
                    gpt5mini_requests: row.get(10)?,
                    gpt5nano_requests: row.get(11)?,
                })
            },
        );

        match result {
            Ok(usage) => Ok(usage),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(DailyUsage {
                date: today,
                ..Default::default()
            }),
            Err(e) => Err(format!("Database query failed: {}", e)),
        }
    }

    /// Increment usage counter atomically
    pub fn increment_request(
        &self,
        user_id: &str,
        model: &str,
        extended_thinking: bool,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<(), String> {
        let conn = acquire_lock(&self.conn);
        let today = Self::today_local();

        // Determine which column to increment
        let model_column = if model.contains("haiku") {
            "haiku_requests"
        } else if model.contains("sonnet") {
            "sonnet_requests"
        } else if model.starts_with("gpt-5.2") {
            "gpt52_requests"
        } else if model.starts_with("gpt-5-mini") {
            "gpt5mini_requests"
        } else if model.starts_with("gpt-5-nano") {
            "gpt5nano_requests"
        } else {
            warn!(model = model, "Unknown model, defaulting to haiku tracking");
            "haiku_requests" // Default to haiku
        };

        // Validate column name against allowlist to prevent SQL injection
        if !VALID_MODEL_COLUMNS.contains(&model_column) {
            return Err(format!("Invalid model column: {}", model_column));
        }

        let thinking_increment: i32 = if extended_thinking { 1 } else { 0 };

        // Upsert with atomic increment
        let sql = format!(
            r#"
            INSERT INTO daily_usage (user_id, date, {model_col}, extended_thinking_requests,
                                     total_input_tokens, total_output_tokens)
            VALUES (?1, ?2, 1, ?3, ?4, ?5)
            ON CONFLICT(user_id, date) DO UPDATE SET
                {model_col} = {model_col} + 1,
                extended_thinking_requests = extended_thinking_requests + ?3,
                total_input_tokens = total_input_tokens + ?4,
                total_output_tokens = total_output_tokens + ?5
            "#,
            model_col = model_column
        );

        conn.execute(
            &sql,
            params![user_id, today, thinking_increment, input_tokens, output_tokens],
        )
        .map_err(|e| format!("Failed to increment usage: {}", e))?;

        debug!(
            model = model_column,
            user = user_id,
            thinking = extended_thinking,
            "Incremented usage"
        );

        Ok(())
    }

    /// Get usage history for the current month
    pub fn get_month_usage(&self, user_id: &str) -> Result<Vec<DailyUsage>, String> {
        let conn = acquire_lock(&self.conn);
        let now = Local::now();
        let month_start = format!("{}-{:02}-01", now.year(), now.month());

        let mut stmt = conn
            .prepare(
                "SELECT date, haiku_requests, sonnet_requests, opus_requests,
                    extended_thinking_requests, total_input_tokens, total_output_tokens,
                    COALESCE(organize_requests, 0), COALESCE(rename_requests, 0),
                    COALESCE(gpt52_requests, 0), COALESCE(gpt5mini_requests, 0), COALESCE(gpt5nano_requests, 0)
             FROM daily_usage
             WHERE user_id = ? AND date >= ?
             ORDER BY date ASC",
            )
            .map_err(|e| format!("Query prepare failed: {}", e))?;

        let usage_iter = stmt
            .query_map(params![user_id, month_start], |row| {
                Ok(DailyUsage {
                    date: row.get(0)?,
                    haiku_requests: row.get(1)?,
                    sonnet_requests: row.get(2)?,
                    opus_requests: row.get(3)?,
                    extended_thinking_requests: row.get(4)?,
                    total_input_tokens: row.get(5)?,
                    total_output_tokens: row.get(6)?,
                    organize_requests: row.get(7)?,
                    rename_requests: row.get(8)?,
                    gpt52_requests: row.get(9)?,
                    gpt5mini_requests: row.get(10)?,
                    gpt5nano_requests: row.get(11)?,
                })
            })
            .map_err(|e| format!("Query failed: {}", e))?;

        usage_iter
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect results: {}", e))
    }

    /// Increment organize request counter atomically
    pub fn increment_organize(&self, user_id: &str) -> Result<(), String> {
        let conn = acquire_lock(&self.conn);
        let today = Self::today_local();

        conn.execute(
            r#"
            INSERT INTO daily_usage (user_id, date, organize_requests)
            VALUES (?1, ?2, 1)
            ON CONFLICT(user_id, date) DO UPDATE SET
                organize_requests = organize_requests + 1
            "#,
            params![user_id, today],
        )
        .map_err(|e| format!("Failed to increment organize usage: {}", e))?;

        debug!(user = user_id, "Incremented organize_requests");
        Ok(())
    }

    /// Increment rename request counter atomically
    pub fn increment_rename(&self, user_id: &str) -> Result<(), String> {
        let conn = acquire_lock(&self.conn);
        let today = Self::today_local();

        conn.execute(
            r#"
            INSERT INTO daily_usage (user_id, date, rename_requests)
            VALUES (?1, ?2, 1)
            ON CONFLICT(user_id, date) DO UPDATE SET
                rename_requests = rename_requests + 1
            "#,
            params![user_id, today],
        )
        .map_err(|e| format!("Failed to increment rename usage: {}", e))?;

        debug!(user = user_id, "Incremented rename_requests");
        Ok(())
    }

    /// Get total token usage for the current month
    ///
    /// Returns (total_input_tokens, total_output_tokens)
    pub fn get_monthly_token_totals(&self, user_id: &str) -> Result<(u64, u64), String> {
        let conn = acquire_lock(&self.conn);
        let now = Local::now();
        let month_start = format!("{}-{:02}-01", now.year(), now.month());

        let result = conn.query_row(
            "SELECT COALESCE(SUM(total_input_tokens), 0), COALESCE(SUM(total_output_tokens), 0)
             FROM daily_usage WHERE user_id = ? AND date >= ?",
            params![user_id, month_start],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
        );

        match result {
            Ok(totals) => Ok(totals),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok((0, 0)),
            Err(e) => Err(format!("Database query failed: {}", e)),
        }
    }

    /// Sync usage from Convex (source of truth)
    /// This allows syncing Convex data back to local SQLite for offline consistency
    pub fn sync_from_convex(
        &self,
        user_id: &str,
        date: &str,
        haiku_requests: u64,
        sonnet_requests: u64,
        extended_thinking_requests: u64,
        gpt52_requests: u64,
        gpt5mini_requests: u64,
        gpt5nano_requests: u64,
    ) -> Result<(), String> {
        let conn = acquire_lock(&self.conn);

        conn.execute(
            r#"
            INSERT INTO daily_usage (user_id, date, haiku_requests, sonnet_requests,
                                     extended_thinking_requests, gpt52_requests,
                                     gpt5mini_requests, gpt5nano_requests)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(user_id, date) DO UPDATE SET
                haiku_requests = MAX(haiku_requests, ?3),
                sonnet_requests = MAX(sonnet_requests, ?4),
                extended_thinking_requests = MAX(extended_thinking_requests, ?5),
                gpt52_requests = MAX(gpt52_requests, ?6),
                gpt5mini_requests = MAX(gpt5mini_requests, ?7),
                gpt5nano_requests = MAX(gpt5nano_requests, ?8)
            "#,
            params![
                user_id,
                date,
                haiku_requests as i64,
                sonnet_requests as i64,
                extended_thinking_requests as i64,
                gpt52_requests as i64,
                gpt5mini_requests as i64,
                gpt5nano_requests as i64,
            ],
        )
        .map_err(|e| format!("Failed to sync usage from Convex: {}", e))?;

        debug!(
            user = user_id,
            date = date,
            haiku = haiku_requests,
            sonnet = sonnet_requests,
            "Synced usage from Convex"
        );

        Ok(())
    }

    /// Clear old usage records (older than 90 days)
    #[allow(dead_code)]
    pub fn cleanup_old_records(&self) -> Result<usize, String> {
        let conn = acquire_lock(&self.conn);
        let cutoff = Utc::now() - chrono::Duration::days(90);
        let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

        let deleted = conn
            .execute(
                "DELETE FROM daily_usage WHERE date < ?",
                params![cutoff_date],
            )
            .map_err(|e| format!("Failed to cleanup old records: {}", e))?;

        Ok(deleted)
    }

    /// Reset all usage for a user (for testing)
    #[cfg(test)]
    pub fn reset_user_usage(&self, user_id: &str) -> Result<(), String> {
        let conn = acquire_lock(&self.conn);
        conn.execute("DELETE FROM daily_usage WHERE user_id = ?", params![user_id])
            .map_err(|e| format!("Failed to reset usage: {}", e))?;
        Ok(())
    }
}

// Note: UsageTracker is automatically Send + Sync because:
// - Mutex<T> is Send when T: Send (Connection is Send)
// - Mutex<T> is Sync when T: Send (Connection is Send)
// The Mutex provides exclusive access, ensuring thread safety.
// No unsafe impl needed - the compiler derives it correctly.

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_tracking() {
        let tracker = UsageTracker::new().unwrap();
        let user_id = "test_user_123";

        // Reset any existing data
        tracker.reset_user_usage(user_id).unwrap();

        // Get initial usage (should be zero)
        let usage = tracker.get_today_usage(user_id).unwrap();
        assert_eq!(usage.haiku_requests, 0);

        // Increment haiku
        tracker
            .increment_request(user_id, "claude-haiku-4-5", false, 100, 50)
            .unwrap();

        // Check updated usage
        let usage = tracker.get_today_usage(user_id).unwrap();
        assert_eq!(usage.haiku_requests, 1);
        assert_eq!(usage.total_input_tokens, 100);
        assert_eq!(usage.total_output_tokens, 50);

        // Increment sonnet with extended thinking
        tracker
            .increment_request(user_id, "claude-sonnet-4-5", true, 200, 100)
            .unwrap();

        // Check updated usage
        let usage = tracker.get_today_usage(user_id).unwrap();
        assert_eq!(usage.sonnet_requests, 1);
        assert_eq!(usage.extended_thinking_requests, 1);

        // Cleanup
        tracker.reset_user_usage(user_id).unwrap();
    }
}
