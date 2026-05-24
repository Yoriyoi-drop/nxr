use crate::error::NxrResult;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct SegmentManager {
    dir: PathBuf,
    current_segment: PathBuf,
    segment_count: u32,
}

impl SegmentManager {
    pub fn new(dir: &Path) -> NxrResult<Self> {
        fs::create_dir_all(dir)?;
        let count = Self::count_segments(dir);
        let seg_name = dir.join(format!("seg_{:03}.bin", count));
        Ok(Self {
            dir: dir.to_path_buf(),
            current_segment: seg_name,
            segment_count: count,
        })
    }

    fn count_segments(dir: &Path) -> u32 {
        let mut count = 0u32;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().into_string().unwrap_or_default();
                if name.starts_with("seg_") && name.ends_with(".bin") {
                    count += 1;
                }
            }
        }
        if count == 0 { 1 } else { count }
    }

    pub fn write_segment(&mut self, id: u64, vector: &[f32], metadata: &[u8]) -> NxrResult<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.current_segment)?;

        let timestamp = chrono::Utc::now().timestamp() as i64;
        let dimension = vector.len() as u32;
        let vec_bytes: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        let meta_len = metadata.len() as u32;

        let mut buf = Vec::new();
        buf.extend_from_slice(&id.to_le_bytes());
        buf.extend_from_slice(&timestamp.to_le_bytes());
        buf.extend_from_slice(&dimension.to_le_bytes());
        buf.extend_from_slice(&vec_bytes);
        buf.extend_from_slice(&meta_len.to_le_bytes());
        buf.extend_from_slice(metadata);

        file.write_all(&buf)?;
        file.flush()?;

        if file.metadata()?.len() > 64 * 1024 * 1024 {
            self.segment_count += 1;
            self.current_segment = self.dir.join(format!("seg_{:03}.bin", self.segment_count));
        }

        Ok(())
    }

    pub fn delete_segment(&self, _id: u64) -> NxrResult<()> {
        Ok(())
    }

    pub fn read_segment(&self, seg_num: u32) -> NxrResult<Vec<(u64, Vec<f32>, Vec<u8>)>> {
        let path = self.dir.join(format!("seg_{:03}.bin", seg_num));
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        let mut records = Vec::new();
        let mut offset = 0;
        while offset + 24 <= data.len() {
            let id = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            let _timestamp = i64::from_le_bytes(data[offset + 8..offset + 16].try_into().unwrap());
            let dim = u32::from_le_bytes(data[offset + 16..offset + 20].try_into().unwrap()) as usize;
            offset += 20;

            let vec_size = dim * 4;
            if offset + vec_size + 4 > data.len() {
                break;
            }
            let mut vector = Vec::with_capacity(dim);
            for i in 0..dim {
                let bytes: [u8; 4] = data[offset + i * 4..offset + (i + 1) * 4].try_into().unwrap();
                vector.push(f32::from_le_bytes(bytes));
            }
            offset += vec_size;

            let meta_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let metadata = data[offset..offset + meta_len].to_vec();
            offset += meta_len;

            records.push((id, vector, metadata));
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_write_read() {
        let dir = std::env::temp_dir().join("nxr_test_seg");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut mgr = SegmentManager::new(&dir).unwrap();
        mgr.write_segment(1, &[0.1, 0.2, 0.3, 0.4], b"meta1").unwrap();
        mgr.write_segment(2, &[0.5, 0.6, 0.7, 0.8], b"meta2").unwrap();

        let records = mgr.read_segment(1).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, 1);
        assert_eq!(records[1].0, 2);
        assert_eq!(records[0].1, vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(records[1].1, vec![0.5, 0.6, 0.7, 0.8]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_segment() {
        let dir = std::env::temp_dir().join("nxr_test_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mgr = SegmentManager::new(&dir).unwrap();
        let result = mgr.read_segment(1);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
