//! redb-backed TIDAL API response cache.

use crate::{TidalError, TidalResult};
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

const TABLE: TableDefinition<&str, &str> = TableDefinition::new("responses");

/// Endpoint cache policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiCachePolicy {
    /// Do not read or write cache.
    NoCache,
    /// Read non-expired cache entries and write fresh responses.
    Ttl(std::time::Duration),
    /// Always fetch fresh and update cache.
    Revalidate,
}

/// Stable cache key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    /// Constructs a stable key from request parts.
    pub fn new(
        method: &str,
        endpoint: &str,
        country_code: Option<&str>,
        params: &[(String, String)],
    ) -> Self {
        let mut params = params.to_vec();
        params.sort();
        let encoded = params
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        Self(format!(
            "{}:{}:{}:{}",
            method,
            endpoint,
            country_code.unwrap_or(""),
            encoded
        ))
    }

    /// Returns the key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Cache entry to write.
#[derive(Debug, Clone)]
pub struct CacheEntryToWrite {
    /// Cache key.
    pub key: CacheKey,
    /// JSON value.
    pub value: serde_json::Value,
    /// Expiration timestamp.
    pub expires_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry {
    expires_at: time::OffsetDateTime,
    value: serde_json::Value,
}

/// API cache abstraction.
#[async_trait::async_trait]
pub trait ApiCache: Send + Sync + std::fmt::Debug {
    /// Reads a cache entry, returning `None` on miss or expiry.
    async fn get(&self, key: &CacheKey) -> TidalResult<Option<serde_json::Value>>;
    /// Writes one cache entry.
    async fn put(&self, entry: CacheEntryToWrite) -> TidalResult<()>;
}

/// redb-backed API cache.
#[derive(Debug, Clone)]
pub struct RedbApiCache {
    path: PathBuf,
    database: Arc<Database>,
}

impl RedbApiCache {
    /// Opens or creates a cache database.
    pub async fn open(path: impl Into<PathBuf>) -> TidalResult<Self> {
        let path = path.into();
        let path_for_open = path.clone();
        let database = tokio::task::spawn_blocking(move || {
            if let Some(parent) = path_for_open.parent() {
                std::fs::create_dir_all(parent).map_err(|error| TidalError::CacheUnavailable {
                    path: path_for_open.display().to_string(),
                    message: error.to_string(),
                })?;
            }
            Database::create(&path_for_open).map_err(|error| TidalError::CacheUnavailable {
                path: path_for_open.display().to_string(),
                message: error.to_string(),
            })
        })
        .await
        .map_err(|error| TidalError::CacheUnavailable {
            path: path.display().to_string(),
            message: error.to_string(),
        })??;
        Ok(Self {
            path,
            database: Arc::new(database),
        })
    }

    /// Writes multiple cache entries in one write transaction.
    pub async fn put_batch(&self, entries: Vec<CacheEntryToWrite>) -> TidalResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let database = Arc::clone(&self.database);
        tokio::task::spawn_blocking(move || {
            let write = database.begin_write().map_err(cache_error)?;
            {
                let mut table = write.open_table(TABLE).map_err(cache_error)?;
                for entry in entries {
                    let stored = StoredEntry {
                        expires_at: entry.expires_at,
                        value: entry.value,
                    };
                    let value =
                        serde_json::to_string(&stored).map_err(|error| TidalError::Cache {
                            message: error.to_string(),
                        })?;
                    table
                        .insert(entry.key.as_str(), value.as_str())
                        .map_err(cache_error)?;
                }
            }
            write.commit().map_err(cache_error)
        })
        .await
        .map_err(|error| TidalError::Cache {
            message: error.to_string(),
        })?
    }

    /// Returns the cache path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[async_trait::async_trait]
impl ApiCache for RedbApiCache {
    async fn get(&self, key: &CacheKey) -> TidalResult<Option<serde_json::Value>> {
        let database = Arc::clone(&self.database);
        let key = key.clone();
        tokio::task::spawn_blocking(move || {
            let read = database.begin_read().map_err(cache_error)?;
            let table = read.open_table(TABLE).map_err(cache_error)?;
            let Some(stored) = table.get(key.as_str()).map_err(cache_error)? else {
                return Ok(None);
            };
            let stored: StoredEntry =
                serde_json::from_str::<StoredEntry>(stored.value()).map_err(|error| {
                    TidalError::Cache {
                        message: error.to_string(),
                    }
                })?;
            if stored.expires_at > time::OffsetDateTime::now_utc() {
                return Ok(Some(stored.value));
            }
            drop(table);
            drop(read);
            let write = database.begin_write().map_err(cache_error)?;
            {
                let mut table = write.open_table(TABLE).map_err(cache_error)?;
                table.remove(key.as_str()).map_err(cache_error)?;
            }
            write.commit().map_err(cache_error)?;
            Ok(None)
        })
        .await
        .map_err(|error| TidalError::Cache {
            message: error.to_string(),
        })?
    }

    async fn put(&self, entry: CacheEntryToWrite) -> TidalResult<()> {
        self.put_batch(vec![entry]).await
    }
}

fn cache_error(error: impl std::fmt::Display) -> TidalError {
    TidalError::Cache {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_sorts_query_params() {
        let left = CacheKey::new(
            "GET",
            "tracks/1",
            Some("US"),
            &[
                ("b".to_owned(), "2".to_owned()),
                ("a".to_owned(), "1".to_owned()),
            ],
        );
        let right = CacheKey::new(
            "GET",
            "tracks/1",
            Some("US"),
            &[
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "2".to_owned()),
            ],
        );
        assert_eq!(left, right);
        assert!(!left.as_str().contains("token"));
    }

    #[tokio::test]
    async fn expired_entry_is_miss() -> TidalResult<()> {
        let dir = tempfile::tempdir().map_err(|error| TidalError::Cache {
            message: error.to_string(),
        })?;
        let cache = RedbApiCache::open(dir.path().join("cache.redb")).await?;
        let key = CacheKey::new("GET", "tracks/1", Some("US"), &[]);
        cache
            .put(CacheEntryToWrite {
                key: key.clone(),
                value: serde_json::json!({ "id": 1 }),
                expires_at: time::OffsetDateTime::now_utc() - time::Duration::seconds(1),
            })
            .await?;
        assert!(cache.get(&key).await?.is_none());
        Ok(())
    }
}
