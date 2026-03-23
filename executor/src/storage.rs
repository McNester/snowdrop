use anyhow::Result;
use async_trait::async_trait;

/// Unified storage backend trait for key-value operations.
/// Implemented by both embedded RocksDB and remote Kvrocks.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn ping(&self) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn mget(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>>;
    async fn mset(&self, pairs: &[(String, Vec<u8>)]) -> Result<()>;
    async fn del(&self, keys: &[String]) -> Result<u64>;
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>>;
    async fn exists(&self, key: &str) -> Result<bool>;
}
