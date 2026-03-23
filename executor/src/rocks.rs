use crate::storage::StorageBackend;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rocksdb::{Options, WriteBatch, DB};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// Embedded RocksDB storage backend. Zero network overhead.
pub struct RocksStorage {
    db: Arc<DB>,
}

impl RocksStorage {
    pub fn open(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_max_open_files(256);
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB write buffer
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024);
        opts.set_level_compaction_dynamic_level_bytes(true);
        // Enable bloom filters for point lookups
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        block_opts.set_cache_index_and_filter_blocks(true);
        opts.set_block_based_table_factory(&block_opts);

        let db = DB::open(&opts, Path::new(path)).context("failed to open RocksDB")?;
        info!(path, "RocksDB opened");

        Ok(Self { db: Arc::new(db) })
    }
}

#[async_trait]
impl StorageBackend for RocksStorage {
    async fn ping(&self) -> Result<()> {
        // Embedded — always alive
        info!("RocksDB PING OK (embedded)");
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let val = self
            .db
            .get(key.as_bytes())
            .context("RocksDB GET failed")?;
        Ok(val)
    }

    async fn mget(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        let byte_keys: Vec<&[u8]> = keys.iter().map(|k| k.as_bytes()).collect();
        let results = self.db.multi_get(&byte_keys);
        results
            .into_iter()
            .map(|r| r.context("RocksDB multi_get failed"))
            .collect()
    }

    async fn mset(&self, pairs: &[(String, Vec<u8>)]) -> Result<()> {
        if pairs.is_empty() {
            return Ok(());
        }
        let mut batch = WriteBatch::default();
        for (key, val) in pairs {
            batch.put(key.as_bytes(), val);
        }
        self.db
            .write(batch)
            .context("RocksDB WriteBatch failed")?;
        Ok(())
    }

    async fn del(&self, keys: &[String]) -> Result<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        let mut batch = WriteBatch::default();
        let mut count = 0u64;
        for key in keys {
            batch.delete(key.as_bytes());
            count += 1;
        }
        self.db
            .write(batch)
            .context("RocksDB DEL batch failed")?;
        Ok(count)
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let prefix_bytes = prefix.as_bytes();
        let mut results = Vec::new();
        let iter = self.db.prefix_iterator(prefix_bytes);
        for item in iter {
            let (key, val) = item.context("RocksDB iterator error")?;
            if !key.starts_with(prefix_bytes) {
                break;
            }
            let key_str =
                String::from_utf8(key.to_vec()).context("non-UTF8 key in RocksDB")?;
            results.push((key_str, val.to_vec()));
        }
        Ok(results)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        // Use key_may_exist for fast path, then confirm with get
        if !self.db.key_may_exist(key.as_bytes()) {
            return Ok(false);
        }
        Ok(self
            .db
            .get(key.as_bytes())
            .context("RocksDB exists check failed")?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rocks_basic_ops() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RocksStorage::open(dir.path().to_str().unwrap()).unwrap();

        // ping
        storage.ping().await.unwrap();

        // mset + get
        storage
            .mset(&[
                ("key1".into(), b"val1".to_vec()),
                ("key2".into(), b"val2".to_vec()),
            ])
            .await
            .unwrap();
        assert_eq!(
            storage.get("key1").await.unwrap(),
            Some(b"val1".to_vec())
        );
        assert_eq!(storage.get("missing").await.unwrap(), None);

        // mget
        let vals = storage
            .mget(&["key1".into(), "missing".into(), "key2".into()])
            .await
            .unwrap();
        assert_eq!(vals[0], Some(b"val1".to_vec()));
        assert_eq!(vals[1], None);
        assert_eq!(vals[2], Some(b"val2".to_vec()));

        // exists
        assert!(storage.exists("key1").await.unwrap());
        assert!(!storage.exists("missing").await.unwrap());

        // del
        let deleted = storage.del(&["key1".into(), "missing".into()]).await.unwrap();
        assert_eq!(deleted, 2); // counts attempted, not found
        assert_eq!(storage.get("key1").await.unwrap(), None);

        // scan_prefix
        storage
            .mset(&[
                ("pfx:a".into(), b"1".to_vec()),
                ("pfx:b".into(), b"2".to_vec()),
                ("other:c".into(), b"3".to_vec()),
            ])
            .await
            .unwrap();
        let scanned = storage.scan_prefix("pfx:").await.unwrap();
        assert_eq!(scanned.len(), 2);
        assert!(scanned.iter().any(|(k, _)| k == "pfx:a"));
        assert!(scanned.iter().any(|(k, _)| k == "pfx:b"));
    }
}
