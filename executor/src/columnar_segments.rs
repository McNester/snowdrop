use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata for one immutable columnar segment.
///
/// Segment data files are introduced in a later read-engine slice; this
/// manifest gives the engine a durable place to record freshness, statistics,
/// and zone maps without relying on whole-table in-memory cache invalidation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnarSegmentManifest {
    pub segment_id: String,
    pub database_id: String,
    pub table_id: String,
    pub schema_version: i64,
    pub row_count: u64,
    pub base_row_version: u64,
    pub max_row_version: u64,
    pub columns: Vec<ColumnSegmentStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnSegmentStats {
    pub column_id: u32,
    pub column_name: String,
    pub type_name: String,
    pub null_count: u64,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
}

pub trait ColumnarSegmentStore: Send + Sync {
    fn write_manifest(&self, manifest: &ColumnarSegmentManifest) -> Result<()>;
    fn read_manifest(&self, segment_id: &str) -> Result<Option<ColumnarSegmentManifest>>;
    fn list_manifests(
        &self,
        database_id: &str,
        table_id: &str,
    ) -> Result<Vec<ColumnarSegmentManifest>>;
}

#[derive(Debug, Clone)]
pub struct FileColumnarSegmentStore {
    root: PathBuf,
}

impl FileColumnarSegmentStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).with_context(|| {
            format!("failed to create columnar segment root {}", root.display())
        })?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn manifest_path(&self, segment_id: &str) -> PathBuf {
        self.root.join(format!("{segment_id}.manifest.json"))
    }
}

impl ColumnarSegmentStore for FileColumnarSegmentStore {
    fn write_manifest(&self, manifest: &ColumnarSegmentManifest) -> Result<()> {
        let path = self.manifest_path(&manifest.segment_id);
        let tmp = path.with_extension("manifest.json.tmp");
        let bytes =
            serde_json::to_vec_pretty(manifest).context("failed to encode segment manifest")?;
        fs::write(&tmp, bytes).with_context(|| {
            format!(
                "failed to write temporary segment manifest {}",
                tmp.display()
            )
        })?;
        fs::rename(&tmp, &path).with_context(|| {
            format!(
                "failed to atomically install segment manifest {}",
                path.display()
            )
        })?;
        Ok(())
    }

    fn read_manifest(&self, segment_id: &str) -> Result<Option<ColumnarSegmentManifest>> {
        let path = self.manifest_path(segment_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read segment manifest {}", path.display()))?;
        let manifest = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to decode segment manifest {}", path.display()))?;
        Ok(Some(manifest))
    }

    fn list_manifests(
        &self,
        database_id: &str,
        table_id: &str,
    ) -> Result<Vec<ColumnarSegmentManifest>> {
        let mut manifests = Vec::new();
        for entry in fs::read_dir(&self.root)
            .with_context(|| format!("failed to list segment root {}", self.root.display()))?
        {
            let entry = entry.context("failed to read segment directory entry")?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read segment manifest {}", path.display()))?;
            let manifest: ColumnarSegmentManifest = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to decode segment manifest {}", path.display()))?;
            if manifest.database_id == database_id && manifest.table_id == table_id {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| a.segment_id.cmp(&b.segment_id));
        Ok(manifests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_and_lists_segment_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileColumnarSegmentStore::open(dir.path()).unwrap();
        let manifest = ColumnarSegmentManifest {
            segment_id: "seg_0001".to_string(),
            database_id: "db".to_string(),
            table_id: "tbl".to_string(),
            schema_version: 1,
            row_count: 10,
            base_row_version: 1,
            max_row_version: 10,
            columns: vec![ColumnSegmentStats {
                column_id: 1,
                column_name: "created_at".to_string(),
                type_name: "DATE".to_string(),
                null_count: 0,
                min_value: Some("2026-01-01".to_string()),
                max_value: Some("2026-01-31".to_string()),
            }],
        };

        store.write_manifest(&manifest).unwrap();

        assert_eq!(
            store.read_manifest("seg_0001").unwrap(),
            Some(manifest.clone())
        );
        assert_eq!(store.list_manifests("db", "tbl").unwrap(), vec![manifest]);
        assert_eq!(store.list_manifests("db", "other").unwrap(), Vec::new());
    }
}
