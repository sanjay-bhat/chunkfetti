//! `chunkfetti build`: world -> tiles at every level of detail -> 3D Tiles tileset.
//!
//! Level 0 tiles are 64 x 64 blocks at full detail. Each level up, a tile covers four tiles of the level below at
//! half the resolution, until one tile covers the whole world. Output (a 3D Tiles 1.1 tileset with glTF content):
//!   <out>/tileset.json
//!   <out>/tiles/<level>/<x>_<z>.glb
//! Working files (each tile's half-resolution voxels, used to build its parent) go to <out>.cache/.

use crate::assets::Assets;
use crate::mesh;
use crate::voxel::{pack, Grid, Half, D, HALF, TILE};
use crate::world::{Chunk, Names, SharedNames, World};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Options {
    pub world: PathBuf,
    pub out: PathBuf,
    pub threads: usize,
    /// world block bounds x0, z0, x1, z1 (inclusive-exclusive); None = the whole world
    pub area: Option<[i32; 4]>,
}

/// What we know about a built tile, for the tileset.
#[derive(Clone, Copy, Debug)]
pub struct TileInfo {
    pub content: bool,
    pub ylo: i32, // block y, inclusive
    pub yhi: i32, // block y, exclusive
}

pub struct Stats {
    pub levels: usize,
    pub tiles: usize,
    pub bytes: u64,
}

pub fn run(o: &Options, assets: &Assets) -> Result<Stats, String> {
    let t0 = Instant::now();
    let world = World::new(&o.world);
    let chunks = world.chunk_list();
    if chunks.is_empty() {
        return Err(format!("no chunks found in {}", world.dir.display()));
    }
    let mut leaves: HashSet<(i32, i32)> = HashSet::new();
    for &(cx, cz) in &chunks {
        let (tx, tz) = (cx.div_euclid(4), cz.div_euclid(4));
        if let Some([x0, z0, x1, z1]) = o.area {
            let (bx, bz) = (tx * TILE as i32, tz * TILE as i32);
            if bx + TILE as i32 <= x0 || bx >= x1 || bz + TILE as i32 <= z0 || bz >= z1 {
                continue;
            }
        }
        leaves.insert((tx, tz));
    }
    // tiles are numbered from the world's lowest corner, so every tile has a common ancestor (the root)
    let origin = (leaves.iter().map(|t| t.0).min().unwrap_or(0), leaves.iter().map(|t| t.1).min().unwrap_or(0));
    let leaves: HashSet<(i32, i32)> = leaves.into_iter().map(|(x, z)| (x - origin.0, z - origin.1)).collect();
    println!("{} chunks, {} level-0 tiles, {} threads", chunks.len(), leaves.len(), o.threads);
    let cache = PathBuf::from(format!("{}.cache", o.out.display()));
    std::fs::create_dir_all(o.out.join("tiles")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let names: SharedNames = Arc::new(Mutex::new(Names::new()));

    // ---- level 0: from the world's blocks, grouped by region so each worker reuses what it decoded
    let mut by_region: BTreeMap<(i32, i32), Vec<(i32, i32)>> = BTreeMap::new();
    for &t in &leaves {
        by_region.entry((t.0.div_euclid(8), t.1.div_euclid(8))).or_default().push(t);
    }
    let jobs: Vec<Vec<(i32, i32)>> = by_region.into_values().collect();
    let infos: Mutex<HashMap<(i32, i32), TileInfo>> = Mutex::new(HashMap::new());
    let bytes = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let next = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        for _ in 0..o.threads {
            sc.spawn(|| {
                let mut w = World::new(&o.world);
                let mut looks: Vec<Option<u32>> = Vec::new();
                let mut decoded: HashMap<(i32, i32), Option<Chunk>> = HashMap::new();
                loop {
                    let j = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(j) else { break };
                    decoded.clear();
                    let mut job = job.clone();
                    job.sort_by_key(|t| (t.1, t.0));
                    for (tx, tz) in job {
                        let g = leaf_grid(&mut w, &mut decoded, &names, assets, &mut looks, origin.0 + tx, origin.1 + tz);
                        let n = finish(&g, 0, tx, tz, origin, &o.out, &cache, &infos);
                        bytes.fetch_add(n, Ordering::Relaxed);
                        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if d % 500 == 0 {
                            println!("level 0: {d}/{} tiles  {:.0}s", leaves.len(), t0.elapsed().as_secs_f32());
                        }
                    }
                }
            });
        }
    });
    let mut levels: Vec<HashMap<(i32, i32), TileInfo>> = vec![infos.into_inner().unwrap()];
    println!("level 0 done: {} tiles  {:.0}s", levels[0].len(), t0.elapsed().as_secs_f32());

    // ---- levels above: each tile from the half-resolution voxels of its four children (plus a border ring)
    let mut cur: HashSet<(i32, i32)> = leaves;
    let mut level = 0usize;
    while cur.len() > 1 || level == 0 {
        level += 1;
        let parents: Vec<(i32, i32)> = cur
            .iter()
            .map(|&(x, z)| (x.div_euclid(2), z.div_euclid(2)))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let infos: Mutex<HashMap<(i32, i32), TileInfo>> = Mutex::new(HashMap::new());
        let next = AtomicUsize::new(0);
        std::thread::scope(|sc| {
            for _ in 0..o.threads {
                sc.spawn(|| loop {
                    let j = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(px, pz)) = parents.get(j) else { break };
                    let g = parent_grid(&cache, level - 1, px, pz);
                    let n = finish(&g, level, px, pz, origin, &o.out, &cache, &infos);
                    bytes.fetch_add(n, Ordering::Relaxed);
                });
            }
        });
        let li = infos.into_inner().unwrap();
        println!("level {level}: {} tiles  {:.0}s", li.len(), t0.elapsed().as_secs_f32());
        levels.push(li);
        cur = parents.into_iter().collect();
        if level > 30 {
            break;
        }
    }
    let n_tiles: usize = levels.iter().map(|l| l.values().filter(|i| i.content).count()).sum();
    write_tileset(&o.out, &levels, origin)?;
    let total = bytes.load(Ordering::Relaxed) as u64;
    println!(
        "done: {} levels, {n_tiles} tiles with content, {:.1} MB, {:.0}s",
        levels.len(),
        total as f64 / 1e6,
        t0.elapsed().as_secs_f32()
    );
    Ok(Stats { levels: levels.len(), tiles: n_tiles, bytes: total })
}

/// Meshes and writes one tile, keeps its half-resolution voxels for the parent, records it. Returns bytes written.
#[allow(clippy::too_many_arguments)]
fn finish(
    g: &Grid,
    level: usize,
    tx: i32,
    tz: i32,
    origin: (i32, i32),
    out: &Path,
    cache: &Path,
    infos: &Mutex<HashMap<(i32, i32), TileInfo>>,
) -> usize {
    let scale = 1i32 << level;
    let (x0, z0) = block_corner(origin, level, tx, tz);
    let m = mesh::mesh(g, x0, z0, scale);
    let mut written = 0;
    if !m.is_empty() {
        let dir = out.join("tiles").join(level.to_string());
        std::fs::create_dir_all(&dir).ok();
        let b = m.glb();
        written = b.len();
        std::fs::write(dir.join(format!("{tx}_{tz}.glb")), b).ok();
    }
    let h = g.half();
    if !h.is_empty() {
        let dir = cache.join(level.to_string());
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join(format!("{tx}_{tz}.bin")), h.to_bytes()).ok();
    }
    let (ylo, yhi) = match g.occupied() {
        Some((lo, hi)) => (lo * scale, (hi + 1) * scale),
        None => (0, 0),
    };
    infos.lock().unwrap().insert((tx, tz), TileInfo { content: !m.is_empty(), ylo, yhi });
    written
}

/// World block x, z of the north-west corner of tile (tx, tz) at `level`.
pub fn block_corner(origin: (i32, i32), level: usize, tx: i32, tz: i32) -> (i64, i64) {
    let t = TILE as i64;
    ((origin.0 as i64 + ((tx as i64) << level)) * t, (origin.1 as i64 + ((tz as i64) << level)) * t)
}

/// Level-0 grid of tile (tx, tz) straight from the world's blocks, with a one-block border.
fn leaf_grid(
    w: &mut World,
    decoded: &mut HashMap<(i32, i32), Option<Chunk>>,
    names: &SharedNames,
    assets: &Assets,
    looks: &mut Vec<Option<u32>>,
    tx: i32,
    tz: i32,
) -> Grid {
    let bx0 = tx * TILE as i32 - 1; // world block x of grid column 0
    let bz0 = tz * TILE as i32 - 1;
    let (c0x, c1x) = (bx0.div_euclid(16), (bx0 + D as i32 - 1).div_euclid(16));
    let (c0z, c1z) = (bz0.div_euclid(16), (bz0 + D as i32 - 1).div_euclid(16));
    let mut sy = (i32::MAX, i32::MIN);
    for cz in c0z..=c1z {
        for cx in c0x..=c1x {
            if !decoded.contains_key(&(cx, cz)) {
                let c = w.chunk(cx, cz, names);
                decoded.insert((cx, cz), c);
            }
            if let Some(Some(c)) = decoded.get(&(cx, cz)) {
                for (y, _) in &c.sections {
                    sy = (sy.0.min(*y), sy.1.max(*y));
                }
            }
        }
    }
    if sy.0 > sy.1 {
        return Grid::new(0, 0);
    }
    let mut g = Grid::new(sy.0 * 16, ((sy.1 - sy.0 + 1) * 16) as usize);
    let name_list: Vec<String> = names.lock().unwrap().list.clone();
    for cz in c0z..=c1z {
        for cx in c0x..=c1x {
            let Some(Some(c)) = decoded.get(&(cx, cz)) else { continue };
            // the part of this chunk inside the grid
            let gx0 = (cx * 16 - bx0).max(0);
            let gx1 = (cx * 16 + 16 - bx0).min(D as i32);
            let gz0 = (cz * 16 - bz0).max(0);
            let gz1 = (cz * 16 + 16 - bz0).min(D as i32);
            for (secy, sec) in &c.sections {
                let ybase = (secy * 16 - g.y0) as usize;
                for ly in 0..16 {
                    for gz in gz0..gz1 {
                        let lz = (gz + bz0 - cz * 16) as usize;
                        for gx in gx0..gx1 {
                            let lx = (gx + bx0 - cx * 16) as usize;
                            let id = sec.get(lx, ly, lz) as usize;
                            if id == 0 {
                                continue;
                            }
                            if looks.len() <= id {
                                looks.resize(id + 1, None);
                            }
                            let v = *looks[id].get_or_insert_with(|| {
                                let l = assets.look(name_list.get(id).map(String::as_str).unwrap_or("minecraft:air"));
                                if l.solid { pack(l.top, l.side) } else { 0 }
                            });
                            if v != 0 {
                                g.set(gx as usize, ybase + ly, gz as usize, v);
                            }
                        }
                    }
                }
            }
        }
    }
    // keep the decoded-chunk cache small: tiles go row by row, so rows above this one are done with
    decoded.retain(|&(_, cz), _| cz >= c0z);
    g
}

/// Grid of tile (px, pz) one level up, from its children's half grids at `child_level` (and their neighbours').
fn parent_grid(cache: &Path, child_level: usize, px: i32, pz: i32) -> Grid {
    let dir = cache.join(child_level.to_string());
    let load = |x: i32, z: i32| -> Option<Half> { Half::from_bytes(&std::fs::read(dir.join(format!("{x}_{z}.bin"))).ok()?) };
    // the 4x4 block of child tiles around this parent: the middle 2x2 are its own, the ring gives the border
    let mut kids: HashMap<(i32, i32), Half> = HashMap::new();
    for j in -1..3 {
        for i in -1..3 {
            if let Some(h) = load(2 * px + i, 2 * pz + j) {
                kids.insert((i, j), h);
            }
        }
    }
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for h in kids.values() {
        lo = lo.min(h.y0);
        hi = hi.max(h.y0 + h.ny as i32);
    }
    if lo >= hi {
        return Grid::new(0, 0);
    }
    let mut g = Grid::new(lo, (hi - lo) as usize);
    for (&(i, j), h) in &kids {
        // grid x of this child's column hx: 1 + i*HALF + hx; keep only what lands in 0..D
        for hz in 0..HALF {
            let gz = 1 + j * HALF as i32 + hz as i32;
            if gz < 0 || gz >= D as i32 {
                continue;
            }
            for hx in 0..HALF {
                let gx = 1 + i * HALF as i32 + hx as i32;
                if gx < 0 || gx >= D as i32 {
                    continue;
                }
                let col = h.column(hx, hz);
                let off = (h.y0 - lo) as usize;
                for (k, &v) in col.iter().enumerate() {
                    if v != 0 {
                        g.set(gx as usize, off + k, gz as usize, v);
                    }
                }
            }
        }
    }
    g
}

/// 3D Tiles 1.1 tileset. Game axes (x east, y up, z south) become the tileset's z-up frame as (x, -z, y),
/// which is exactly how glTF's y-up content is turned z-up by 3D Tiles readers.
fn write_tileset(out: &Path, levels: &[HashMap<(i32, i32), TileInfo>], origin: (i32, i32)) -> Result<(), String> {
    type Levels = [HashMap<(i32, i32), TileInfo>];
    fn node(levels: &Levels, origin: (i32, i32), level: usize, tx: i32, tz: i32) -> Option<(serde_json::Value, i32, i32)> {
        let info = levels[level].get(&(tx, tz)).copied();
        let mut children = Vec::new();
        let (mut ylo, mut yhi) = match info {
            Some(i) if i.yhi > i.ylo => (i.ylo, i.yhi),
            _ => (i32::MAX, i32::MIN),
        };
        if level > 0 {
            for j in 0..2 {
                for i in 0..2 {
                    if let Some((c, l, h)) = node(levels, origin, level - 1, 2 * tx + i, 2 * tz + j) {
                        ylo = ylo.min(l);
                        yhi = yhi.max(h);
                        children.push(c);
                    }
                }
            }
        }
        let content = info.map(|i| i.content).unwrap_or(false);
        if !content && children.is_empty() {
            return None;
        }
        let size = (TILE as i64) << level;
        let (x0, z0) = block_corner(origin, level, tx, tz);
        let (cx, cz) = (x0 as f64 + size as f64 / 2.0, z0 as f64 + size as f64 / 2.0);
        let cy = (ylo as f64 + yhi as f64) / 2.0;
        let (h, hy) = (size as f64 / 2.0, ((yhi - ylo) as f64 / 2.0).max(0.5));
        let mut n = serde_json::json!({
            "boundingVolume": {"box": [cx, -cz, cy, h, 0, 0, 0, h, 0, 0, 0, hy]},
            "geometricError": if level == 0 { 0.0 } else { (1u64 << level) as f64 },
            "refine": "REPLACE"
        });
        if content {
            n["content"] = serde_json::json!({"uri": format!("tiles/{level}/{tx}_{tz}.glb")});
        }
        if !children.is_empty() {
            n["children"] = serde_json::Value::Array(children);
        }
        Some((n, ylo, yhi))
    }
    let top = levels.len() - 1;
    let roots: Vec<(i32, i32)> = levels[top].keys().copied().collect();
    let mut rs: Vec<serde_json::Value> = roots.iter().filter_map(|&(x, z)| node(levels, origin, top, x, z).map(|n| n.0)).collect();
    let root = if rs.len() == 1 {
        rs.pop().unwrap()
    } else {
        return Err("more than one root tile".into());
    };
    let ge = (1u64 << (top + 1)) as f64;
    let ts = serde_json::json!({
        "asset": {"version": "1.1", "generator": "chunkfetti"},
        "geometricError": ge,
        "root": root
    });
    std::fs::write(out.join("tileset.json"), serde_json::to_vec(&ts).unwrap()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::{self, Tag};
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;

    /// Writes a tiny world in code: a 3x2-chunk stone floor with one tall pillar.
    fn tiny_world(dir: &Path) {
        std::fs::create_dir_all(dir.join("region")).unwrap();
        std::fs::write(dir.join("level.dat"), b"test").unwrap();
        let mut region = vec![0u8; 8192];
        let mut sector = 2usize;
        for cz in 0..2 {
            for cx in 0..3 {
                // section 0: bottom layer stone; pillar at (5, *, 5) in chunk (1, 1)
                let mut idx = vec![0u16; 4096];
                for i in 0..256 {
                    idx[i] = 1;
                }
                if cx == 1 && cz == 1 {
                    for y in 0..16 {
                        idx[(y << 8) | (5 << 4) | 5] = 1;
                    }
                }
                let mut longs = vec![0i64; 256];
                for (i, &v) in idx.iter().enumerate() {
                    longs[i / 16] |= (v as i64) << ((i % 16) * 4);
                }
                let sec = Tag::Compound(vec![
                    ("Y".into(), Tag::Byte(0)),
                    (
                        "block_states".into(),
                        Tag::Compound(vec![
                            (
                                "palette".into(),
                                Tag::List(vec![
                                    Tag::Compound(vec![("Name".into(), Tag::String("minecraft:air".into()))]),
                                    Tag::Compound(vec![("Name".into(), Tag::String("test:stone".into()))]),
                                ]),
                            ),
                            ("data".into(), Tag::LongArray(longs)),
                        ]),
                    ),
                ]);
                let root = Tag::Compound(vec![("sections".into(), Tag::List(vec![sec]))]);
                let mut raw = Vec::new();
                nbt::write(&root, &mut raw);
                let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
                z.write_all(&raw).unwrap();
                let body = z.finish().unwrap();
                let mut blob = ((body.len() + 1) as u32).to_be_bytes().to_vec();
                blob.push(2);
                blob.extend_from_slice(&body);
                let n = blob.len().div_ceil(4096);
                blob.resize(n * 4096, 0);
                let e = (cz * 32 + cx) * 4;
                region[e] = (sector >> 16) as u8;
                region[e + 1] = (sector >> 8) as u8;
                region[e + 2] = sector as u8;
                region[e + 3] = n as u8;
                region.extend_from_slice(&blob);
                sector += n;
            }
        }
        std::fs::write(dir.join("region/r.0.0.mca"), &region).unwrap();
        // the same chunks again west of x = 0, so the world crosses zero like real worlds do
        std::fs::write(dir.join("region/r.-1.0.mca"), &region).unwrap();
    }

    #[test]
    fn builds_a_tileset_from_a_tiny_world() {
        let base = std::env::temp_dir().join(format!("chunkfetti-build-{}", std::process::id()));
        let world = base.join("world");
        tiny_world(&world);
        let out = base.join("out");
        let o = Options { world: world.clone(), out: out.clone(), threads: 2, area: None };
        let s = run(&o, &Assets::default()).unwrap();
        assert!(s.tiles >= 2, "level 0 and at least one parent");
        assert!(s.levels <= 6, "tiles either side of x = 0 must share a root (got {} levels)", s.levels);
        let ts: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("tileset.json")).unwrap()).unwrap();
        assert_eq!(ts["asset"]["version"], "1.1");
        assert!(out.join("tiles/0/0_0.glb").is_file());
        let root = &ts["root"];
        let half_width = root["boundingVolume"]["box"][3].as_f64().unwrap();
        assert!(half_width >= 512.0, "root covers both regions");
        std::fs::remove_dir_all(&base).ok();
    }
}
