//! TT Games `.DAT` archive ("PakDat"), the container the Android `.obb` actually is.
//!
//! The archive is opened read-only and files are pulled out on demand; nothing is ever written
//! back to it.

use crate::inflate::inflate;
use crate::{Error, Reader, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub position: u64,
    pub packed_size: u32,
    pub size: u32,
    pub mode: i32,
}

pub struct Archive {
    file: Mutex<File>,
    entries: Vec<Entry>,
    /// Lower-case path with `\` separators -> index into `entries`.
    index: HashMap<String, usize>,
    names: Vec<String>,
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path.as_ref())
            .map_err(|e| Error::new(format!("cannot open archive {}: {e}", path.as_ref().display())))?;
        let mut head = [0u8; 8];
        file.read_exact(&mut head)?;
        let raw_offset = i32::from_le_bytes(head[0..4].try_into().unwrap()) as i64;
        let dir_offset = if raw_offset < 0 { raw_offset * -0x100 } else { raw_offset } as u64;
        let dir_size = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
        let mut dir = vec![0u8; dir_size];
        file.seek(SeekFrom::Start(dir_offset))?;
        file.read_exact(&mut dir)?;

        let mut r = Reader::new(&dir);
        let version = r.i32()?;
        let count = r.i32()? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = r.u32()? as u64;
            let packed_size = r.u32()?;
            let size = r.u32()?;
            let mode = r.i32()?;
            let position = if version < -1 { offset << 8 } else { offset };
            entries.push(Entry { position, packed_size, size, mode });
        }
        let node_count = r.i32()? as usize;
        let node_size = if version > -5 { 8 } else { 12 };
        let nodes_at = r.pos;
        r.pos += node_count * node_size;
        let names_size = r.u32()? as usize;
        let names = r.bytes(names_size)?;
        let node = |i: usize| -> Result<(i16, i16, usize)> {
            let mut n = Reader::new(&dir);
            n.pos = nodes_at + i * node_size;
            Ok((n.i16()?, n.i16()?, n.u32()? as usize))
        };
        let name_at = |offset: usize| -> String {
            let end = names[offset..].iter().position(|&b| b == 0).map_or(names.len(), |p| offset + p);
            names[offset..end].iter().map(|&b| b as char).collect()
        };

        // Walk the name tree depth-first. A node whose child is <= 0 is a file (record -child).
        let mut index = HashMap::with_capacity(count);
        let mut paths = vec![String::new(); count];
        let mut stack: Vec<(usize, String)> = Vec::new();
        if node_count > 0 {
            let (child, _, _) = node(0)?;
            if child > 0 {
                stack.push((child as usize, String::new()));
            }
        }
        while let Some((mut i, prefix)) = stack.pop() {
            loop {
                let (child, sibling, name_offset) = node(i)?;
                let full = format!("{prefix}{}", name_at(name_offset));
                if child > 0 {
                    stack.push((child as usize, format!("{full}\\")));
                } else {
                    let record = (-(child as i32)) as usize;
                    if record < count {
                        index.insert(full.to_ascii_lowercase(), record);
                        paths[record] = full;
                    }
                }
                if sibling <= 0 {
                    break;
                }
                i = sibling as usize;
            }
        }
        Ok(Self { file: Mutex::new(file), entries, index, names: paths })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every path in the archive, in record order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(|s| s.as_str()).filter(|s| !s.is_empty())
    }

    fn key(path: &str) -> String {
        path.replace('/', "\\").to_ascii_lowercase()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&Self::key(path))
    }

    pub fn entry(&self, path: &str) -> Option<Entry> {
        self.index.get(&Self::key(path)).map(|&i| self.entries[i])
    }

    /// Read and decompress one file.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let entry = self.entry(path).ok_or_else(|| Error::new(format!("not in archive: {path}")))?;
        let mut packed = vec![0u8; entry.packed_size as usize];
        {
            let mut file = self.file.lock().unwrap();
            file.seek(SeekFrom::Start(entry.position))?;
            file.read_exact(&mut packed)?;
        }
        match entry.mode {
            0 => Ok(packed),
            3 => {
                let mut out = Vec::with_capacity(entry.size as usize);
                let mut r = Reader::new(&packed);
                while out.len() < entry.size as usize {
                    if r.bytes(4)? != b"DFLT" {
                        return Err(Error::new(format!("{path}: missing DFLT block header")));
                    }
                    let block_packed = r.u32()? as usize;
                    let block_size = r.u32()? as usize;
                    let data = r.bytes(block_packed)?;
                    if block_packed == block_size {
                        out.extend_from_slice(data);
                    } else {
                        let start = out.len();
                        let mut block = Vec::with_capacity(block_size);
                        inflate(data, &mut block)?;
                        if block.len() != block_size {
                            return Err(Error::new(format!(
                                "{path}: block at {start} inflated to {} bytes, expected {block_size}",
                                block.len()
                            )));
                        }
                        out.extend_from_slice(&block);
                    }
                }
                Ok(out)
            }
            mode => Err(Error::new(format!("{path}: unsupported compression mode {mode}"))),
        }
    }

    pub fn read_text(&self, path: &str) -> Result<String> {
        Ok(self.read(path)?.iter().map(|&b| b as char).collect())
    }
}
