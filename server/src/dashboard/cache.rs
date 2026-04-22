//! TTL cache for pre-serialized Arrow IPC chart responses.
//!
//! The cache stores each chart's most recent response body as `Arc<Vec<u8>>`
//! so request handlers hand back cheap clones rather than re-serializing on
//! every hit. Entries are keyed by chart id; there is no eviction beyond
//! expiry-driven refresh, because the catalog is bounded by the configured
//! chart list.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use tokio::sync::RwLock;

/// A single cache entry: the generation timestamp and the pre-serialized
/// Arrow IPC bytes for a chart's query result.
#[derive(Debug, Clone)]
pub struct CachedArrow {
    /// Wall-clock time the entry was produced. Surfaced to clients via the
    /// `X-Tiders-Generated-At` response header in commit 3.
    pub generated_at: SystemTime,
    /// Serialized Arrow IPC stream body. `Arc` so cache hits are a clone of
    /// the handle, not a copy of the bytes.
    pub ipc_bytes: Arc<Vec<u8>>,
}

impl CachedArrow {
    fn is_fresh(&self, ttl: Duration, now: SystemTime) -> bool {
        now.duration_since(self.generated_at)
            .map(|age| age < ttl)
            .unwrap_or(false)
    }
}

/// Returns a cache entry for `id`, fetching and storing one if the current
/// entry is missing or older than `ttl`.
///
/// Concurrency model: a read lock checks for a fresh entry; on miss, a write
/// lock is acquired and the staleness check is repeated so a concurrent task
/// that just populated the entry does not trigger a redundant fetch. The
/// provided `fetch_fn` runs under the write lock; this is intentional MVP
/// stampede protection. It means a slow fetch for chart A blocks cache
/// lookups for unrelated chart B — acceptable given small catalogs.
pub async fn get_or_fetch<F, Fut>(
    cache: &RwLock<HashMap<String, CachedArrow>>,
    id: &str,
    ttl: Duration,
    fetch_fn: F,
) -> Result<CachedArrow>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    let now = SystemTime::now();

    {
        let read = cache.read().await;
        if let Some(entry) = read.get(id)
            && entry.is_fresh(ttl, now)
        {
            tracing::debug!(chart_id = %id, "dashboard cache hit");
            return Ok(entry.clone());
        }
    }

    let mut write = cache.write().await;

    // Re-check under the write lock: another task may have populated the
    // entry while this task was blocked on lock acquisition.
    let now = SystemTime::now();
    if let Some(entry) = write.get(id)
        && entry.is_fresh(ttl, now)
    {
        tracing::debug!(chart_id = %id, "dashboard cache hit (after wait)");
        return Ok(entry.clone());
    }

    tracing::debug!(chart_id = %id, "dashboard cache miss — fetching");
    let bytes = fetch_fn().await?;
    let entry = CachedArrow {
        generated_at: SystemTime::now(),
        ipc_bytes: Arc::new(bytes),
    };
    write.insert(id.to_string(), entry.clone());
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn caches_and_returns_same_bytes_within_ttl() {
        let cache = RwLock::new(HashMap::new());
        let calls = AtomicUsize::new(0);

        let first = get_or_fetch(&cache, "c1", Duration::from_secs(60), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1, 2, 3])
        })
        .await
        .unwrap();

        let second = get_or_fetch(&cache, "c1", Duration::from_secs(60), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![9, 9, 9])
        })
        .await
        .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(*first.ipc_bytes, vec![1, 2, 3]);
        assert_eq!(*second.ipc_bytes, vec![1, 2, 3]);
        assert_eq!(first.generated_at, second.generated_at);
    }

    #[tokio::test]
    async fn refetches_after_ttl_expires() {
        let cache = RwLock::new(HashMap::new());
        let calls = AtomicUsize::new(0);

        get_or_fetch(&cache, "c1", Duration::from_millis(10), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1])
        })
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(25)).await;

        let refreshed = get_or_fetch(&cache, "c1", Duration::from_millis(10), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![2])
        })
        .await
        .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(*refreshed.ipc_bytes, vec![2]);
    }

    #[tokio::test]
    async fn fetch_error_does_not_poison_cache() {
        let cache = RwLock::new(HashMap::new());

        let err = get_or_fetch(&cache, "c1", Duration::from_secs(60), || async {
            Err(anyhow::anyhow!("boom"))
        })
        .await;
        assert!(err.is_err());

        let ok = get_or_fetch(&cache, "c1", Duration::from_secs(60), || async { Ok(vec![7]) })
            .await
            .unwrap();
        assert_eq!(*ok.ipc_bytes, vec![7]);
    }
}
