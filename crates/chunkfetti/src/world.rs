//! Reads a saved world: region files (r.X.Z.mca) and the block palettes of each 16x16 chunk (1.18+ format).

use crate::nbt::{self, Tag};
use flate2::read::{GzDecoder, ZlibDecoder};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Global block-name table: id 0 is air; every other name gets a small number.
#[derive(Default)]
pub struct Names {
    pub list: Vec<String>,
    index: HashMap<String, u16>,
}

impl Names {
    pub fn new() -> Self {
        let mut n = Names::default();
        n.id("minecraft:air");
        n
    }
    pub fn id(&mut self, name: &str) -> u16 {
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        let i = self.list.len() as u16;
        self.list.push(name.to_string());
        self.index.insert(name.to_string(), i);
        i
    }
}

pub type SharedNames = Arc<Mutex<Names>>;

/// One 16x16x16 section: block ids indexed y*256 + z*16 + x, or a single id for the whole section.
pub enum Section {
    Uniform(u16),
    Mixed(Box<[u16; 4096]>),
}

impl Section {
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u16 {
        match self {
            Section::Uniform(v) => *v,
            Section::Mixed(a) => a[(y << 8) | (z << 4) | x],
        }
    }
}

/// A chunk: its sections by section y (block y = sy * 16 ..).
pub struct Chunk {
    pub sections: Vec<(i32, Section)>,
}

pub struct Region {
    data: Vec<u8>,
}

impl Region {
    pub fn open(path: &Path) -> std::io::Result<Region> {
        Ok(Region { data: std::fs::read(path)? })
    }

    /// Which of the 32x32 chunks this region holds.
    pub fn present(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        if self.data.len() < 4096 {
            return out;
        }
        for i in 0..1024 {
            let e = &self.data[i * 4..i * 4 + 4];
            if e != [0, 0, 0, 0] {
                out.push(((i % 32) as i32, (i / 32) as i32));
            }
        }
        out
    }

    /// The raw NBT of chunk (lx, lz) inside this region.
    pub fn chunk_nbt(&self, lx: i32, lz: i32) -> Option<Tag> {
        let i = ((lz & 31) * 32 + (lx & 31)) as usize * 4;
        if self.data.len() < 4096 {
            return None;
        }
        let e = &self.data[i..i + 4];
        let off = ((e[0] as usize) << 16 | (e[1] as usize) << 8 | e[2] as usize) * 4096;
        if off == 0 || off + 5 > self.data.len() {
            return None;
        }
        let len = u32::from_be_bytes(self.data[off..off + 4].try_into().unwrap()) as usize;
        let kind = self.data[off + 4];
        let end = (off + 4 + len).min(self.data.len());
        let body = &self.data[off + 5..end];
        let mut raw = Vec::new();
        let ok = match kind {
            1 => GzDecoder::new(body).read_to_end(&mut raw).is_ok(),
            2 => ZlibDecoder::new(body).read_to_end(&mut raw).is_ok(),
            3 => {
                raw.extend_from_slice(body);
                true
            }
            _ => false, // external .mcc chunks and other codecs: not supported yet
        };
        if !ok {
            return None;
        }
        nbt::parse(&raw).ok()
    }
}

/// Decodes a chunk's block sections into block ids.
pub fn decode_chunk(root: &Tag, names: &SharedNames) -> Chunk {
    let mut sections = Vec::new();
    let secs = root.get("sections").and_then(Tag::as_list).unwrap_or(&[]);
    for s in secs {
        let Some(sy) = s.get("Y").and_then(Tag::as_i64) else { continue };
        let Some(bs) = s.get("block_states") else { continue };
        let pal = bs.get("palette").and_then(Tag::as_list).unwrap_or(&[]);
        if pal.is_empty() {
            continue;
        }
        let ids: Vec<u16> = {
            let mut n = names.lock().unwrap();
            pal.iter().map(|p| n.id(p.get("Name").and_then(Tag::as_str).unwrap_or("minecraft:air"))).collect()
        };
        let data = bs.get("data").and_then(Tag::as_longs);
        let sec = match data {
            None => Section::Uniform(ids[0]),
            Some(longs) => {
                let bits = (usize::BITS - (ids.len() - 1).leading_zeros()).max(4) as usize;
                let per = 64 / bits;
                let mask = (1u64 << bits) - 1;
                let mut a = Box::new([0u16; 4096]);
                for (i, slot) in a.iter_mut().enumerate() {
                    let l = longs.get(i / per).copied().unwrap_or(0) as u64;
                    let v = ((l >> ((i % per) * bits)) & mask) as usize;
                    *slot = *ids.get(v).unwrap_or(&0);
                }
                Section::Mixed(a)
            }
        };
        if let Section::Uniform(0) = sec {
            continue; // all air
        }
        sections.push((sy as i32, sec));
    }
    Chunk { sections }
}

/// A world on disk: its region folder and a small cache of open region files.
pub struct World {
    pub dir: PathBuf,
    cache: HashMap<(i32, i32), Option<Arc<Region>>>,
}

impl World {
    pub fn new(world: &Path) -> World {
        World { dir: world.join("region"), cache: HashMap::new() }
    }

    /// All chunk positions in the world, from the region file headers (fast: no chunk is decoded).
    pub fn chunk_list(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return out };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let p: Vec<&str> = n.split('.').collect();
            if p.len() != 4 || p[0] != "r" || p[3] != "mca" {
                continue;
            }
            let (Ok(rx), Ok(rz)) = (p[1].parse::<i32>(), p[2].parse::<i32>()) else { continue };
            if let Ok(r) = Region::open(&e.path()) {
                for (lx, lz) in r.present() {
                    out.push((rx * 32 + lx, rz * 32 + lz));
                }
            }
        }
        out.sort();
        out
    }

    fn region(&mut self, rx: i32, rz: i32) -> Option<Arc<Region>> {
        if self.cache.len() > 12 {
            self.cache.clear();
        }
        self.cache
            .entry((rx, rz))
            .or_insert_with(|| Region::open(&self.dir.join(format!("r.{rx}.{rz}.mca"))).ok().map(Arc::new))
            .clone()
    }

    pub fn chunk(&mut self, cx: i32, cz: i32, names: &SharedNames) -> Option<Chunk> {
        let r = self.region(cx >> 5, cz >> 5)?;
        let t = r.chunk_nbt(cx & 31, cz & 31)?;
        Some(decode_chunk(&t, names))
    }
}
