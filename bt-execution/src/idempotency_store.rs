use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{SqlitePool, Row};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// SQLite-backed persistent idempotency store.
/// Survives process restarts and enforces TTL-based deduplication.
pub struct IdempotencyStore {
    pool: SqlitePool,
    ttl_seconds: i64,
}

impl IdempotencyStore {
    /// Create a new idempotency store at the given database path.
    /// Runs schema initialization on first creation.
    pub async fn new(db_path: &str, ttl_seconds: i64) -> Result<Self> {
        // Ensure the parent directory exists (relative paths like "data/..." need it)
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Use path-based options (not a `sqlite://` URL) so absolute Windows paths
        // like `C:\Users\...` are not misinterpreted as a URL host.
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;

        // Initialize schema
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS idempotency_keys (
                client_order_id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL  -- ISO8601 UTC
            )
            "#,
        )
        .execute(&pool)
        .await?;

        info!("Idempotency store initialized at {}", db_path);

        Ok(Self { pool, ttl_seconds })
    }

    /// Check if a client_order_id exists and insert it atomically.
    /// Returns `true` if duplicate (already exists), `false` if newly inserted.
    pub async fn check_and_insert(
        &self,
        client_order_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<bool> {
        let ts_str = timestamp.to_rfc3339();

        // Try to insert; if conflict, it's a duplicate
        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO idempotency_keys (client_order_id, timestamp)
            VALUES (?, ?)
            "#,
        )
        .bind(client_order_id)
        .bind(ts_str)
        .execute(&self.pool)
        .await?;

        let is_duplicate = result.rows_affected() == 0;

        if is_duplicate {
            debug!("Idempotency duplicate detected: {}", client_order_id);
        } else {
            debug!("Idempotency key inserted: {}", client_order_id);
        }

        Ok(is_duplicate)
    }

    /// Remove all entries older than TTL.
    pub async fn purge_expired(&self) -> Result<()> {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.ttl_seconds);
        let cutoff_str = cutoff.to_rfc3339();

        let result = sqlx::query("DELETE FROM idempotency_keys WHERE timestamp < ?")
            .bind(cutoff_str)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() > 0 {
            info!("Purged {} expired idempotency keys", result.rows_affected());
        }

        Ok(())
    }

    /// Load all keys within the TTL window into an in-memory HashMap.
    /// Called on startup to warm the cache.
    pub async fn load_active(&self) -> Result<HashMap<String, DateTime<Utc>>> {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.ttl_seconds);
        let cutoff_str = cutoff.to_rfc3339();

        let rows = sqlx::query(
            "SELECT client_order_id, timestamp FROM idempotency_keys WHERE timestamp >= ?",
        )
        .bind(cutoff_str)
        .fetch_all(&self.pool)
        .await?;

        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            let key: String = row.get("client_order_id");
            let ts_str: String = row.get("timestamp");
            if let Ok(ts) = DateTime::parse_from_rfc3339(&ts_str) {
                map.insert(key, ts.with_timezone(&Utc));
            }
        }

        info!("Loaded {} active idempotency keys from persistent store", map.len());
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_idempotency_store_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_idempotency.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let store = IdempotencyStore::new(db_path.to_str().unwrap(), 300).await.unwrap();

        let now = Utc::now();
        let key = "TEST-IDEMPOTENCY-001";

        // First insert should succeed (not duplicate)
        let is_dup = store.check_and_insert(key, now).await.unwrap();
        assert!(!is_dup, "First insert should not be duplicate");

        // Second insert with same key should be detected as duplicate
        let is_dup = store.check_and_insert(key, now).await.unwrap();
        assert!(is_dup, "Second insert should be duplicate");

        // Load active should return the key
        let active = store.load_active().await.unwrap();
        assert!(active.contains_key(key));
        assert_eq!(active.len(), 1);

        // Purge expired (should not remove since it's fresh)
        store.purge_expired().await.unwrap();
        let active = store.load_active().await.unwrap();
        assert_eq!(active.len(), 1);

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_idempotency_store_purge_expired() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_idempotency_purge.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let store = IdempotencyStore::new(db_path.to_str().unwrap(), 1).await.unwrap(); // 1 second TTL

        let past_time = Utc::now() - chrono::Duration::seconds(10);
        let key = "OLD-KEY";
        store.check_and_insert(key, past_time).await.unwrap();

        let active = store.load_active().await.unwrap();
        assert_eq!(active.len(), 0); // Already expired

        let now = Utc::now();
        store.check_and_insert("NEW-KEY", now).await.unwrap();
        let active = store.load_active().await.unwrap();
        assert_eq!(active.len(), 1);
        assert!(active.contains_key("NEW-KEY"));

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }
}