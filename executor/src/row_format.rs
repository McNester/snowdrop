/// Zero-copy row format for RocksDB storage.
///
/// Binary layout:
/// ```text
/// [version: u8][num_cols: u16][col_id: u32, offset: u32, len: u32]...[raw bytes]
/// ```
///
/// Version byte `0x01` identifies this format. Absence of `0x01` as the first byte
/// triggers fallback to legacy bincode deserialization.
/// All multi-byte integers are little-endian.

const FORMAT_VERSION: u8 = 0xFF;
const HEADER_SIZE: usize = 3; // version(1) + num_cols(2)
const ENTRY_SIZE: usize = 10; // col_id(4) + offset(4) + len(2)

/// Encode columns into the zero-copy binary format.
pub fn encode_row(columns: &[(u32, &[u8])]) -> Vec<u8> {
    let num_cols = columns.len();
    let data_size: usize = columns.iter().map(|(_, v)| v.len()).sum();
    let total = HEADER_SIZE + num_cols * ENTRY_SIZE + data_size;

    let mut buf = Vec::with_capacity(total);

    // Header
    buf.push(FORMAT_VERSION);
    buf.extend_from_slice(&(num_cols as u16).to_le_bytes());

    // Directory entries — offsets are relative to start of data region
    let mut offset = 0u32;
    for (col_id, val) in columns {
        buf.extend_from_slice(&col_id.to_le_bytes());
        buf.extend_from_slice(&offset.to_le_bytes());
        buf.extend_from_slice(&(val.len() as u16).to_le_bytes());
        offset += val.len() as u32;
    }

    // Data region
    for (_, val) in columns {
        buf.extend_from_slice(val);
    }

    buf
}

/// Zero-copy view over a serialized row. Borrows the underlying byte slice.
pub struct RowView<'a> {
    data: &'a [u8],
    num_cols: u16,
    data_offset: usize,
}

impl<'a> RowView<'a> {
    /// Parse a row from raw bytes. Supports both the new format (version 0x01)
    /// and legacy bincode format (fallback).
    /// Returns None if the data is legacy bincode and must be handled separately.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        if data.is_empty() || data[0] != FORMAT_VERSION {
            return None;
        }
        if data.len() < HEADER_SIZE {
            return None;
        }
        let num_cols = u16::from_le_bytes([data[1], data[2]]);
        let data_offset = HEADER_SIZE + (num_cols as usize) * ENTRY_SIZE;
        if data.len() < data_offset {
            return None;
        }
        Some(Self {
            data,
            num_cols,
            data_offset,
        })
    }

    pub fn num_cols(&self) -> u16 {
        self.num_cols
    }

    /// Get col_id for the directory entry at `index`.
    fn entry_col_id(&self, index: usize) -> u32 {
        let base = HEADER_SIZE + index * ENTRY_SIZE;
        u32::from_le_bytes([
            self.data[base],
            self.data[base + 1],
            self.data[base + 2],
            self.data[base + 3],
        ])
    }

    /// Get offset and length for the directory entry at `index`.
    fn entry_range(&self, index: usize) -> (u32, u16) {
        let base = HEADER_SIZE + index * ENTRY_SIZE + 4;
        let offset = u32::from_le_bytes([
            self.data[base],
            self.data[base + 1],
            self.data[base + 2],
            self.data[base + 3],
        ]);
        let len = u16::from_le_bytes([self.data[base + 4], self.data[base + 5]]);
        (offset, len)
    }

    /// Get the raw bytes for the column at directory `index`.
    pub fn col_bytes_at(&self, index: usize) -> &'a [u8] {
        let (offset, len) = self.entry_range(index);
        let start = self.data_offset + offset as usize;
        &self.data[start..start + len as usize]
    }

    /// Find column bytes by col_id. Linear scan over the directory.
    pub fn find_col(&self, col_id: u32) -> Option<&'a [u8]> {
        for i in 0..self.num_cols as usize {
            if self.entry_col_id(i) == col_id {
                return Some(self.col_bytes_at(i));
            }
        }
        None
    }

    /// Iterate over all (col_id, value_bytes) pairs without allocation.
    pub fn iter(&'a self) -> RowViewIter<'a> {
        RowViewIter {
            view: self,
            index: 0,
        }
    }

    /// Convert to owned Vec for write-path read-modify-write operations.
    pub fn to_owned_columns(&self) -> Vec<(u32, Vec<u8>)> {
        self.iter()
            .map(|(id, bytes)| (id, bytes.to_vec()))
            .collect()
    }
}

pub struct RowViewIter<'a> {
    view: &'a RowView<'a>,
    index: usize,
}

impl<'a> Iterator for RowViewIter<'a> {
    type Item = (u32, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.view.num_cols as usize {
            return None;
        }
        let col_id = self.view.entry_col_id(self.index);
        let bytes = self.view.col_bytes_at(self.index);
        self.index += 1;
        Some((col_id, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let columns: Vec<(u32, &[u8])> = vec![(1, b"hello"), (2, b"world"), (3, b"!")];
        let encoded = encode_row(&columns);
        let view = RowView::new(&encoded).expect("should parse new format");

        assert_eq!(view.num_cols(), 3);
        assert_eq!(view.find_col(1), Some(b"hello".as_ref()));
        assert_eq!(view.find_col(2), Some(b"world".as_ref()));
        assert_eq!(view.find_col(3), Some(b"!".as_ref()));
        assert_eq!(view.find_col(99), None);
    }

    #[test]
    fn empty_row() {
        let encoded = encode_row(&[]);
        let view = RowView::new(&encoded).unwrap();
        assert_eq!(view.num_cols(), 0);
        assert_eq!(view.find_col(1), None);
        assert_eq!(view.iter().count(), 0);
    }

    #[test]
    fn single_column() {
        let encoded = encode_row(&[(42, b"data")]);
        let view = RowView::new(&encoded).unwrap();
        assert_eq!(view.num_cols(), 1);
        assert_eq!(view.find_col(42), Some(b"data".as_ref()));
    }

    #[test]
    fn empty_value() {
        let encoded = encode_row(&[(1, b""), (2, b"notempty")]);
        let view = RowView::new(&encoded).unwrap();
        assert_eq!(view.find_col(1), Some(b"".as_ref()));
        assert_eq!(view.find_col(2), Some(b"notempty".as_ref()));
    }

    #[test]
    fn iterator_yields_all_columns() {
        let columns: Vec<(u32, &[u8])> = vec![(10, b"a"), (20, b"bb"), (30, b"ccc")];
        let encoded = encode_row(&columns);
        let view = RowView::new(&encoded).unwrap();

        let collected: Vec<(u32, &[u8])> = view.iter().collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], (10, b"a".as_ref()));
        assert_eq!(collected[1], (20, b"bb".as_ref()));
        assert_eq!(collected[2], (30, b"ccc".as_ref()));
    }

    #[test]
    fn to_owned_columns_matches() {
        let columns: Vec<(u32, &[u8])> = vec![(1, b"x"), (2, b"yy")];
        let encoded = encode_row(&columns);
        let view = RowView::new(&encoded).unwrap();

        let owned = view.to_owned_columns();
        assert_eq!(owned, vec![(1, b"x".to_vec()), (2, b"yy".to_vec())]);
    }

    #[test]
    fn legacy_bincode_returns_none() {
        let legacy = bincode::serialize(&vec![(1u32, b"val".to_vec())]).unwrap();
        assert!(RowView::new(&legacy).is_none());
    }

    #[test]
    fn large_values() {
        let big = vec![0xABu8; 60_000];
        let encoded = encode_row(&[(1, &big)]);
        let view = RowView::new(&encoded).unwrap();
        assert_eq!(view.find_col(1).unwrap().len(), 60_000);
        assert!(view.find_col(1).unwrap().iter().all(|&b| b == 0xAB));
    }
}
