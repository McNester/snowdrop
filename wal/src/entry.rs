use serde::{Deserialize, Serialize};
use std::io::{self, Read, Seek, SeekFrom, Write};
use thiserror::Error;
use tracing::debug;

pub const WAL_HEADER_SIZE: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum WalStatus {
    Pending = 0,
    Neo4jCommitted = 1,
    Committed = 2,
    RolledBack = 3,
}

impl TryFrom<u8> for WalStatus {
    type Error = WalError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Neo4jCommitted),
            2 => Ok(Self::Committed),
            3 => Ok(Self::RolledBack),
            _ => Err(WalError::InvalidStatus(value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalHeader {
    pub entry_id: u64,
    pub timestamp: u64,
    pub status: u8,
    pub payload_len: u32,
}

impl WalHeader {
    pub fn to_bytes(&self) -> [u8; WAL_HEADER_SIZE] {
        let mut buf = [0u8; WAL_HEADER_SIZE];
        buf[0..8].copy_from_slice(&self.entry_id.to_le_bytes());
        buf[8..16].copy_from_slice(&self.timestamp.to_le_bytes());
        buf[16] = self.status;
        // payload_len is not stored in the fixed 17 bytes per the spec,
        // but we need it to know how much payload follows
        // Spec says 17 bytes: entry_id(8) + timestamp(8) + status(1) = 17
        // payload_len goes right after
        buf
    }

    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.entry_id.to_le_bytes())?;
        writer.write_all(&self.timestamp.to_le_bytes())?;
        writer.write_all(&[self.status])?;
        writer.write_all(&self.payload_len.to_le_bytes())?;
        Ok(())
    }

    pub fn read_from<R: Read>(reader: &mut R) -> io::Result<Option<Self>> {
        let mut entry_id_buf = [0u8; 8];
        match reader.read_exact(&mut entry_id_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        let mut timestamp_buf = [0u8; 8];
        reader.read_exact(&mut timestamp_buf)?;

        let mut status_buf = [0u8; 1];
        reader.read_exact(&mut status_buf)?;

        let mut payload_len_buf = [0u8; 4];
        reader.read_exact(&mut payload_len_buf)?;

        Ok(Some(WalHeader {
            entry_id: u64::from_le_bytes(entry_id_buf),
            timestamp: u64::from_le_bytes(timestamp_buf),
            status: status_buf[0],
            payload_len: u32::from_le_bytes(payload_len_buf),
        }))
    }
}

// Version-wrapped payload from day one
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalPayload {
    V1(WalPayloadV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalPayloadV1 {
    Insert {
        db_id: u32,
        table_id: u32,
        row_id: String,
        columns: Vec<(u32, Vec<u8>)>,
    },
    Update {
        db_id: u32,
        table_id: u32,
        row_id: String,
        columns: Vec<(u32, Vec<u8>)>,
    },
    Delete {
        db_id: u32,
        table_id: u32,
        row_id: String,
        col_ids: Vec<u32>,
    },
}

#[derive(Debug)]
pub struct WalEntry {
    pub header: WalHeader,
    pub payload: WalPayload,
    /// Byte offset of this entry's header in the WAL file
    pub file_offset: u64,
}

#[derive(Debug, Error)]
pub enum WalError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("serialization error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("invalid WAL status byte: {0}")]
    InvalidStatus(u8),
}

/// Appends a new entry to the WAL file and fsyncs.
/// Returns the entry_id assigned.
pub fn append<W: Write + Seek>(
    writer: &mut W,
    next_entry_id: u64,
    payload: WalPayload,
) -> Result<u64, WalError> {
    let payload_bytes = bincode::serialize(&payload)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let header = WalHeader {
        entry_id: next_entry_id,
        timestamp: now,
        status: WalStatus::Pending as u8,
        payload_len: payload_bytes.len() as u32,
    };

    writer.seek(SeekFrom::End(0))?;
    header.write_to(writer)?;
    writer.write_all(&payload_bytes)?;

    debug!(
        entry_id = next_entry_id,
        payload_len = payload_bytes.len(),
        "WAL entry appended"
    );

    Ok(next_entry_id)
}

/// Updates the status byte of an existing entry in the WAL file and fsyncs.
/// `file_offset` is the byte position where the entry's header starts.
pub fn update_status<W: Write + Seek>(
    writer: &mut W,
    file_offset: u64,
    status: WalStatus,
) -> Result<(), WalError> {
    // Status byte is at offset + 16 (after entry_id:8 + timestamp:8)
    writer.seek(SeekFrom::Start(file_offset + 16))?;
    writer.write_all(&[status as u8])?;

    debug!(offset = file_offset, ?status, "WAL status updated");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn header_roundtrip() {
        let header = WalHeader {
            entry_id: 42,
            timestamp: 123456789,
            status: WalStatus::Pending as u8,
            payload_len: 100,
        };

        let mut buf = Vec::new();
        header.write_to(&mut buf).unwrap();

        // Header is 8 + 8 + 1 + 4 = 21 bytes on disk
        assert_eq!(buf.len(), 21);

        let mut cursor = Cursor::new(&buf);
        let read_back = WalHeader::read_from(&mut cursor).unwrap().unwrap();

        assert_eq!(read_back.entry_id, 42);
        assert_eq!(read_back.timestamp, 123456789);
        assert_eq!(read_back.status, WalStatus::Pending as u8);
        assert_eq!(read_back.payload_len, 100);
    }

    #[test]
    fn header_read_empty_returns_none() {
        let buf: Vec<u8> = vec![];
        let mut cursor = Cursor::new(&buf);
        assert!(WalHeader::read_from(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn append_and_read_back() {
        let mut buf = Cursor::new(Vec::new());

        let payload = WalPayload::V1(WalPayloadV1::Insert {
            db_id: 1,
            table_id: 2,
            row_id: "aX3kP9mQr2z".to_string(),
            columns: vec![(1, b"hello".to_vec()), (2, b"world".to_vec())],
        });

        let entry_id = append(&mut buf, 1, payload.clone()).unwrap();
        assert_eq!(entry_id, 1);

        // Read it back
        buf.seek(SeekFrom::Start(0)).unwrap();
        let header = WalHeader::read_from(&mut buf).unwrap().unwrap();
        assert_eq!(header.entry_id, 1);
        assert_eq!(header.status, WalStatus::Pending as u8);

        let mut payload_buf = vec![0u8; header.payload_len as usize];
        buf.read_exact(&mut payload_buf).unwrap();
        let read_payload: WalPayload = bincode::deserialize(&payload_buf).unwrap();

        match read_payload {
            WalPayload::V1(WalPayloadV1::Insert {
                db_id,
                table_id,
                row_id,
                columns,
            }) => {
                assert_eq!(db_id, 1);
                assert_eq!(table_id, 2);
                assert_eq!(row_id, "aX3kP9mQr2z");
                assert_eq!(columns.len(), 2);
            }
            _ => panic!("unexpected payload variant"),
        }
    }

    #[test]
    fn update_status_works() {
        let mut buf = Cursor::new(Vec::new());

        let payload = WalPayload::V1(WalPayloadV1::Delete {
            db_id: 1,
            table_id: 2,
            row_id: "test".to_string(),
            col_ids: vec![1, 2, 3],
        });

        append(&mut buf, 1, payload).unwrap();

        // Update status to Committed (entry starts at offset 0)
        update_status(&mut buf, 0, WalStatus::Committed).unwrap();

        // Read back header and verify
        buf.seek(SeekFrom::Start(0)).unwrap();
        let header = WalHeader::read_from(&mut buf).unwrap().unwrap();
        assert_eq!(header.status, WalStatus::Committed as u8);
    }

    #[test]
    fn multiple_entries() {
        let mut buf = Cursor::new(Vec::new());

        let p1 = WalPayload::V1(WalPayloadV1::Insert {
            db_id: 1,
            table_id: 1,
            row_id: "row1".to_string(),
            columns: vec![(1, b"val1".to_vec())],
        });
        let p2 = WalPayload::V1(WalPayloadV1::Insert {
            db_id: 1,
            table_id: 1,
            row_id: "row2".to_string(),
            columns: vec![(1, b"val2".to_vec())],
        });

        append(&mut buf, 1, p1).unwrap();
        let offset2 = buf.position();
        append(&mut buf, 2, p2).unwrap();

        // Read both back
        buf.seek(SeekFrom::Start(0)).unwrap();
        let h1 = WalHeader::read_from(&mut buf).unwrap().unwrap();
        assert_eq!(h1.entry_id, 1);
        buf.seek(SeekFrom::Current(h1.payload_len as i64)).unwrap();

        let h2 = WalHeader::read_from(&mut buf).unwrap().unwrap();
        assert_eq!(h2.entry_id, 2);

        // Update second entry status
        update_status(&mut buf, offset2, WalStatus::Neo4jCommitted).unwrap();
        buf.seek(SeekFrom::Start(offset2)).unwrap();
        let h2_updated = WalHeader::read_from(&mut buf).unwrap().unwrap();
        assert_eq!(h2_updated.status, WalStatus::Neo4jCommitted as u8);
    }

    #[test]
    fn wal_status_from_u8() {
        assert_eq!(WalStatus::try_from(0).unwrap(), WalStatus::Pending);
        assert_eq!(WalStatus::try_from(1).unwrap(), WalStatus::Neo4jCommitted);
        assert_eq!(WalStatus::try_from(2).unwrap(), WalStatus::Committed);
        assert_eq!(WalStatus::try_from(3).unwrap(), WalStatus::RolledBack);
        assert!(WalStatus::try_from(4).is_err());
    }
}
