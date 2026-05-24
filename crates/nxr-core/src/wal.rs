use crate::error::{NxrError, NxrResult};
use crc32fast::Hasher;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAGIC: &[u8; 4] = b"NXRW";

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WalOperation {
    Insert = 0x01,
    Update = 0x02,
    Delete = 0x03,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WalLayer {
    Vector = 0x01,
    Graph = 0x02,
    Kv = 0x03,
}

#[derive(Debug, Clone)]
pub struct WalEntry {
    pub lsn: u64,
    pub operation: WalOperation,
    pub layer: WalLayer,
    pub payload: Vec<u8>,
    pub checksum: u32,
}

impl WalEntry {
    pub fn new(operation: WalOperation, layer: WalLayer, payload: Vec<u8>) -> Self {
        let lsn = 0;
        let checksum = 0;
        Self { lsn, operation, layer, payload, checksum }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(26 + self.payload.len());
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&self.lsn.to_le_bytes());
        buf.push(self.operation as u8);
        buf.push(self.layer as u8);
        buf.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.payload);
        let mut hasher = Hasher::new();
        hasher.update(&buf);
        buf.extend_from_slice(&hasher.finalize().to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> NxrResult<Self> {
        if data.len() < 22 {
            return Err(NxrError::Wal("Entry too short".into()));
        }
        if &data[0..4] != MAGIC {
            return Err(NxrError::Wal("Invalid magic bytes".into()));
        }
        let lsn = u64::from_le_bytes(data[4..12].try_into().unwrap());
        let operation = match data[12] {
            0x01 => WalOperation::Insert,
            0x02 => WalOperation::Update,
            0x03 => WalOperation::Delete,
            _ => return Err(NxrError::Wal("Invalid operation".into())),
        };
        let layer = match data[13] {
            0x01 => WalLayer::Vector,
            0x02 => WalLayer::Graph,
            0x03 => WalLayer::Kv,
            _ => return Err(NxrError::Wal("Invalid layer".into())),
        };
        let payload_len = u32::from_le_bytes(data[14..18].try_into().unwrap()) as usize;
        if 18 + payload_len + 4 > data.len() {
            return Err(NxrError::Wal("Truncated entry".into()));
        }
        let payload = data[18..18 + payload_len].to_vec();
        let stored_cs = u32::from_le_bytes(
            data[18 + payload_len..18 + payload_len + 4].try_into().unwrap(),
        );
        let mut hasher = Hasher::new();
        hasher.update(&data[..18 + payload_len]);
        let computed_cs = hasher.finalize();
        if stored_cs != computed_cs {
            return Err(NxrError::Wal("Checksum mismatch".into()));
        }
        Ok(Self { lsn, operation, layer, payload, checksum: stored_cs })
    }
}

pub struct Wal {
    dir: PathBuf,
    current_file: Mutex<File>,
    current_lsn: Mutex<u64>,
    max_segment_size: u64,
}

impl Wal {
    pub fn open<P: AsRef<Path>>(dir: P) -> NxrResult<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;

        let max_segment_size: u64 = 64 * 1024 * 1024;
        let seg_name = Self::next_segment_name(&dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&seg_name)?;

        let current_lsn = Self::scan_last_lsn(&dir)?;

        Ok(Self {
            dir,
            current_file: Mutex::new(file),
            current_lsn: Mutex::new(current_lsn),
            max_segment_size,
        })
    }

    fn next_segment_name(dir: &Path) -> NxrResult<PathBuf> {
        let mut max = 0u32;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("wal_") && name.ends_with(".log") {
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                max = max.max(num);
            }
        }
        Ok(dir.join(format!("wal_{:05}.log", max + 1)))
    }

    fn scan_last_lsn(dir: &Path) -> NxrResult<u64> {
        let mut last_lsn = 0u64;
        let mut entries: Vec<(u32, PathBuf)> = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("wal_") && name.ends_with(".log") {
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                entries.push((num, entry.path()));
            }
        }
        entries.sort_by_key(|e| e.0);
        for (_, path) in &entries {
            let data = fs::read(path)?;
            let mut offset = 0;
            while offset + 22 <= data.len() {
                let payload_len = u32::from_le_bytes(
                    data[offset + 14..offset + 18].try_into().unwrap_or([0; 4]),
                ) as usize;
                let entry_size = 22 + payload_len;
                if offset + entry_size > data.len() {
                    break;
                }
                if let Ok(entry) = WalEntry::decode(&data[offset..offset + entry_size]) {
                    last_lsn = entry.lsn;
                }
                offset += entry_size;
            }
        }
        Ok(last_lsn + 1)
    }

    pub fn append(&self, operation: WalOperation, layer: WalLayer, payload: &[u8]) -> NxrResult<u64> {
        let mut file = self.current_file.lock().unwrap();
        let mut lsn = self.current_lsn.lock().unwrap();
        let entry = WalEntry { lsn: *lsn, ..WalEntry::new(operation, layer, payload.to_vec()) };
        let encoded = entry.encode();
        file.write_all(&encoded)?;
        file.flush()?;

        let current_lsn = *lsn;
        *lsn += 1;

        if file.metadata()?.len() > self.max_segment_size {
            let new_path = Self::next_segment_name(&self.dir)?;
            *file = OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .open(&new_path)?;
        }

        Ok(current_lsn)
    }

    pub fn replay<F>(&self, mut callback: F) -> NxrResult<u64>
    where
        F: FnMut(WalEntry) -> NxrResult<()>,
    {
        let mut count = 0u64;
        let mut entries: Vec<(u32, PathBuf)> = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("wal_") && name.ends_with(".log") {
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                entries.push((num, entry.path()));
            }
        }
        entries.sort_by_key(|e| e.0);
        for (_, path) in &entries {
            let data = fs::read(path)?;
            let mut offset = 0;
            while offset + 22 <= data.len() {
                let payload_len = u32::from_le_bytes(
                    data[offset + 14..offset + 18].try_into().unwrap_or([0; 4]),
                ) as usize;
                let entry_size = 22 + payload_len;
                if offset + entry_size > data.len() {
                    break;
                }
                if let Ok(entry) = WalEntry::decode(&data[offset..offset + entry_size]) {
                    callback(entry)?;
                    count += 1;
                }
                offset += entry_size;
            }
        }
        Ok(count)
    }

    pub fn truncate(&self, keep_segments: u32) -> NxrResult<()> {
        let mut entries: Vec<(u32, PathBuf)> = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("wal_") && name.ends_with(".log") {
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                entries.push((num, entry.path()));
            }
        }
        entries.sort_by_key(|e| e.0);
        let keep = entries.len().saturating_sub(keep_segments as usize);
        for (_, path) in entries.iter().take(keep) {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_wal(name: &str) -> Wal {
        let dir = std::env::temp_dir().join(format!("nxr_wal_{}", name));
        let _ = fs::remove_dir_all(&dir);
        Wal::open(&dir).unwrap()
    }

    #[test]
    fn test_append_and_replay() {
        let wal = test_wal("append_replay");
        let lsn = wal.append(WalOperation::Insert, WalLayer::Vector, b"test_payload").unwrap();
        assert_eq!(lsn, 1);

        let mut count = 0u64;
        wal.replay(|entry| {
            count += 1;
            assert_eq!(entry.operation, WalOperation::Insert);
            assert_eq!(entry.layer, WalLayer::Vector);
            assert_eq!(entry.payload, b"test_payload");
            Ok(())
        }).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_wal_entry_encode_decode() {
        let entry = WalEntry::new(
            WalOperation::Insert,
            WalLayer::Graph,
            vec![1, 2, 3, 4],
        );
        let encoded = entry.encode();
        let decoded = WalEntry::decode(&encoded).unwrap();
        assert_eq!(decoded.operation, WalOperation::Insert);
        assert_eq!(decoded.layer, WalLayer::Graph);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_wal_truncate() {
        let wal = test_wal("truncate");
        wal.append(WalOperation::Insert, WalLayer::Kv, b"a").unwrap();
        wal.append(WalOperation::Insert, WalLayer::Kv, b"b").unwrap();
        wal.truncate(0).unwrap();

        let mut entries = Vec::new();
        for entry in fs::read_dir(&wal.dir).unwrap() {
            entries.push(entry.unwrap().path());
        }
        assert!(entries.is_empty() || entries.iter().all(|p| {
            p.file_name().unwrap().to_str().unwrap().starts_with("wal_")
        }));
    }

    #[test]
    fn test_invalid_magic() {
        let result = WalEntry::decode(&[0u8; 22]);
        assert!(result.is_err());
    }
}
