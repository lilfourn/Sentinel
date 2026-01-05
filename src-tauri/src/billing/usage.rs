//! SQLite-backed usage tracking
//!
//! Tracks daily API usage per model with atomic increments.
//! Data persists locally and can be synced with Convex.

use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

use super::types::DailyUsage;

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
                PRIMARY KEY (user_id, date)
            );

            CREATE INDEX IF NOT EXISTS idx_usage_date
                ON daily_usage(user_id, date DESC);
        "#,
        )
        .map_err(|e| format!("Failed to create tables: {}", e))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn get_db_path() -> Result<PathBuf, String> {
        dirs::config_dir()
            .map(|d| d.join("sentinel").join("usage.db"))
            .ok_or_else(|| "Could not determine config directory".to_string())
    }

    /// Get today's UTC date string
    fn today_utc() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    /// Get usage for today
    pub fn get_today_usage(&self, user_id: &str) -> Result<DailyUsage, String> {
        let conn = self.conn.lock().unwrap();
        let today = Self::today_utc();

        let result = conn.query_row(
            "SELECT date, haiku_requests, sonnet_requests, opus_requests,
                    extended_thinking_requests, total_input_tokens, total_output_tokens
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
        let conn = self.conn.lock().unwrap();
        let today = Self::today_utc();

        // Determine which column to increment
        let model_column = if model.contains("haiku") {
            "haiku_requests"
        } else if model.contains("sonnet") {
            "sonnet_requests"
        } else if model.contains("opus") {
            "opus_requests"
        } else {
            "haiku_requests" // Default to haiku
        };

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

        eprintln!(
            "[UsageTracker] Incremented {} for user {} (thinking: {})",
            model_column, user_id, extended_thinking
        );

        Ok(())
    }

    /// Get usage history for the current month
    pub fn get_month_usage(&self, user_id: &str) -> Result<Vec<DailyUsage>, String> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let month_start = format!("{}-{:02}-01", now.year(), now.month());

        let mut stmt = conn
            .prepare(
                "SELECT date, haiku_requests, sonnet_requests, opus_requests,
                    extended_thinking_requests, total_input_tokens, total_output_tokens
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
                })
            })
            .map_err(|e| format!("Query failed: {}", e))?;

        usage_iter
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect results: {}", e))
    }

    /// Get total token usage for the current month
    ///
    /// Returns (total_input_tokens, total_output_tokens)
    pub fn get_monthly_token_totals(&self, user_id: &str) -> Result<(u64, u64), String> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
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

    /// Clear old usage records (older than 90 days)
    #[allow(dead_code)]
    pub fn cleanup_old_records(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM daily_usage WHERE user_id = ?", params![user_id])
            .map_err(|e| format!("Failed to reset usage: {}", e))?;
        Ok(())
    }
}

// Implement Send + Sync for thread safety
unsafe impl Send for UsageTracker {}
unsafe impl Sync for UsageTracker {}

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
