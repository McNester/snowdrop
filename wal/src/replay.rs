use crate::entry::{WalEntry, WalError, WalHeader, WalPayload, WalStatus};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub struct WalFile {
    path: PathBuf,
    next_entry_id: u64,
}

impl WalFile {
    /// Opens or creates the WAL file at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WalError> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create file if it doesn't exist
        OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&path)?;

        info!(path = %path.display(), "WAL file opened");

        Ok(Self {
            path,
            next_entry_id: 1,
        })
    }

    /// Scans the WAL file and returns all entries.
    /// Sets next_entry_id to max(entry_id) + 1.
    pub fn scan_all(&mut self) -> Result<Vec<WalEntry>, WalError> {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut max_id = 0u64;

        loop {
            let file_offset = reader.seek(SeekFrom::Current(0))?;

            let header = match WalHeader::read_from(&mut reader)? {
                Some(h) => h,
                None => break,
            };

            let mut payload_buf = vec![0u8; header.payload_len as usize];
            reader.read_exact(&mut payload_buf)?;

            let payload: WalPayload = bincode::deserialize(&payload_buf)?;

            if header.entry_id > max_id {
                max_id = header.entry_id;
            }

            entries.push(WalEntry {
                header,
                payload,
                file_offset,
            });
        }

        self.next_entry_id = max_id + 1;
        debug!(count = entries.len(), next_id = self.next_entry_id, "WAL scan complete");

        Ok(entries)
    }

    /// Performs startup replay:
    /// - Returns entries in Neo4jCommitted state (need Kvrocks re-execution)
    /// - Marks Pending entries as RolledBack
    pub fn replay(&mut self) -> Result<Vec<WalEntry>, WalError> {
        let all = self.scan_all()?;
        let mut needs_replay = Vec::new();
        let mut rollback_offsets = Vec::new();

        for entry in &all {
            let status = WalStatus::try_from(entry.header.status)?;
            match status {
                WalStatus::Neo4jCommitted => {
                    info!(entry_id = entry.header.entry_id, "entry needs Kvrocks replay");
                    needs_replay.push(WalEntry {
                        header: entry.header.clone(),
                        payload: entry.payload.clone(),
                        file_offset: entry.file_offset,
                    });
                }
                WalStatus::Pending => {
                    warn!(entry_id = entry.header.entry_id, "marking Pending entry as RolledBack");
                    rollback_offsets.push(entry.file_offset);
                }
                WalStatus::Committed | WalStatus::RolledBack => {}
            }
        }

        // Mark Pending entries as RolledBack
        if !rollback_offsets.is_empty() {
            let file = OpenOptions::new().write(true).open(&self.path)?;
            let mut writer = BufWriter::new(file);
            for offset in rollback_offsets {
                crate::entry::update_status(&mut writer, offset, WalStatus::RolledBack)?;
            }
            writer.flush()?;
        }

        Ok(needs_replay)
    }

    /// Appends a new entry to the WAL.
    pub fn append(&mut self, payload: WalPayload) -> Result<(u64, u64), WalError> {
        let file = OpenOptions::new()
            .write(true)
            .append(false)
            .open(&self.path)?;
        let mut writer = BufWriter::new(file);

        let file_offset = writer.seek(SeekFrom::End(0))?;
        let entry_id = self.next_entry_id;
        crate::entry::append(&mut writer, entry_id, payload)?;
        writer.flush()?;

        self.next_entry_id += 1;
        Ok((entry_id, file_offset))
    }

    /// Updates entry status at a given file offset.
    pub fn update_status(&self, file_offset: u64, status: WalStatus) -> Result<(), WalError> {
        let file = OpenOptions::new().write(true).open(&self.path)?;
        let mut writer = BufWriter::new(file);
        crate::entry::update_status(&mut writer, file_offset, status)?;
        writer.flush()?;
        Ok(())
    }

    /// Compacts the WAL by removing entries older than `max_age_secs` in terminal states
    /// (Committed or RolledBack). Rewrites the file with only active/recent entries.
    pub fn compact(&mut self, max_age_secs: u64) -> Result<usize, WalError> {
        let entries = self.scan_all()?;
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        let cutoff_nanos = now_nanos.saturating_sub(max_age_secs * 1_000_000_000);

        let mut keep = Vec::new();
        let mut removed = 0usize;

        for entry in &entries {
            let status = WalStatus::try_from(entry.header.status)?;
            let is_terminal = matches!(status, WalStatus::Committed | WalStatus::RolledBack);
            let is_old = entry.header.timestamp < cutoff_nanos;

            if is_terminal && is_old {
                removed += 1;
            } else {
                keep.push(entry);
            }
        }

        if removed == 0 {
            return Ok(0);
        }

        // Write surviving entries to a temp file, then rename
        let tmp_path = self.path.with_extension("compact.tmp");
        {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp_path)?;
            let mut writer = BufWriter::new(file);

            for entry in &keep {
                entry.header.write_to(&mut writer)?;
                let payload_bytes = bincode::serialize(&entry.payload)?;
                writer.write_all(&payload_bytes)?;
            }
            writer.flush()?;
        }

        std::fs::rename(&tmp_path, &self.path)?;

        // Re-scan to fix offsets and next_entry_id
        self.scan_all()?;

        info!(removed, kept = keep.len(), "WAL compaction complete");
        Ok(removed)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{WalPayloadV1, WalStatus};
    use tempfile::TempDir;

    fn test_payload() -> WalPayload {
        WalPayload::V1(WalPayloadV1::Insert {
            db_id: 1,
            table_id: 2,
            row_id: "testrow12345".to_string(),
            columns: vec![(1, b"value".to_vec())],
        })
    }

    #[test]
    fn open_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wal.log");
        assert!(!path.exists());

        let _wal = WalFile::open(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn open_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("dir").join("wal.log");

        let _wal = WalFile::open(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn append_and_scan() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wal.log");
        let mut wal = WalFile::open(&path).unwrap();

        wal.append(test_payload()).unwrap();
        wal.append(test_payload()).unwrap();

        let entries = wal.scan_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].header.entry_id, 1);
        assert_eq!(entries[1].header.entry_id, 2);
    }

    #[test]
    fn replay_marks_pending_as_rolledback() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wal.log");
        let mut wal = WalFile::open(&path).unwrap();

        // Append two entries (both Pending)
        wal.append(test_payload()).unwrap();
        wal.append(test_payload()).unwrap();

        // Replay should mark both as RolledBack
        let needs_replay = wal.replay().unwrap();
        assert!(needs_replay.is_empty());

        // Verify they are now RolledBack
        let entries = wal.scan_all().unwrap();
        for e in &entries {
            assert_eq!(e.header.status, WalStatus::RolledBack as u8);
        }
    }

    #[test]
    fn replay_returns_neo4j_committed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wal.log");
        let mut wal = WalFile::open(&path).unwrap();

        let (_, offset1) = wal.append(test_payload()).unwrap();
        wal.append(test_payload()).unwrap();

        // Mark first entry as Neo4jCommitted
        wal.update_status(offset1, WalStatus::Neo4jCommitted).unwrap();

        let needs_replay = wal.replay().unwrap();
        assert_eq!(needs_replay.len(), 1);
        assert_eq!(needs_replay[0].header.entry_id, 1);

        // Second entry should be RolledBack
        let entries = wal.scan_all().unwrap();
        assert_eq!(entries[1].header.status, WalStatus::RolledBack as u8);
    }

    #[test]
    fn replay_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wal.log");
        let mut wal = WalFile::open(&path).unwrap();

        let needs_replay = wal.replay().unwrap();
        assert!(needs_replay.is_empty());
    }

    #[test]
    fn entry_id_monotonic_across_reopens() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wal.log");

        {
            let mut wal = WalFile::open(&path).unwrap();
            wal.append(test_payload()).unwrap();
            wal.append(test_payload()).unwrap();
        }

        // Reopen and scan to recover next_entry_id
        let mut wal = WalFile::open(&path).unwrap();
        wal.scan_all().unwrap();
        let (entry_id, _) = wal.append(test_payload()).unwrap();
        assert_eq!(entry_id, 3);
    }
}
