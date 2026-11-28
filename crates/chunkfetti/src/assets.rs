//! Block colours, read from the user's own game jar, mod jars and resource packs at build time.
//! Nothing here is shipped with chunkfetti: the files are opened where the user installed them.
//!
//! For each block name: blockstate JSON -> first model -> textures (following model parents) -> average colour of
//! the top and side textures, tinted like a temperate biome where the game tints them (grass, leaves, water).

use flate2::read::{DeflateDecoder, ZlibDecoder};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// What a block looks like from far away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub solid: bool,
    pub top: [u8; 3],
    pub side: [u8; 3],
}

enum Source {
    Zip { data: std::sync::Arc<Vec<u8>>, offset: usize, method: u16, csize: usize },
    File(PathBuf),
}

/// Every `assets/...` file from the game jar, mods and resource packs (later sources override earlier ones).
#[derive(Default)]
pub struct Assets {
    files: HashMap<String, Source>,
    pub sources: Vec<String>,
}

impl Assets {
    /// Adds a jar or zip (game, mod or resource pack) or an unpacked resource-pack folder.
    pub fn add(&mut self, path: &Path) -> Result<usize, String> {
        if path.is_dir() {
            let mut n = 0;
            let mut stack = vec![path.to_path_buf()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).map_err(|e| e.to_string())?.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if let Ok(rel) = p.strip_prefix(path) {
                        let rel = rel.to_string_lossy().replace('\\', "/");
                        if rel.starts_with("assets/") {
                            self.files.insert(rel, Source::File(p));
                            n += 1;
                        }
                    }
                }
            }
            self.sources.push(path.display().to_string());
            return Ok(n);
        }
        let data = std::sync::Arc::new(std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?);
        let entries = zip_entries(&data).ok_or_else(|| format!("{}: not a zip/jar", path.display()))?;
        let mut n = 0;
        for (name, offset, method, csize) in entries {
            if name.starts_with("assets/") && !name.ends_with('/') {
                self.files.insert(name, Source::Zip { data: data.clone(), offset, method, csize });
                n += 1;
            }
        }
        self.sources.push(path.display().to_string());
        Ok(n)
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        match self.files.get(name)? {
            Source::File(p) => std::fs::read(p).ok(),
            Source::Zip { data, offset, method, csize } => {
                let d = &data[..];
                let o = *offset;
                if o + 30 > d.len() || d[o..o + 4] != [0x50, 0x4b, 0x03, 0x04] {
                    return None;
                }
                let nl = u16::from_le_bytes([d[o + 26], d[o + 27]]) as usize;
                let el = u16::from_le_bytes([d[o + 28], d[o + 29]]) as usize;
                let start = o + 30 + nl + el;
                let body = d.get(start..start + csize)?;
                match method {
                    0 => Some(body.to_vec()),
                    8 => {
                        let mut out = Vec::new();
                        DeflateDecoder::new(body).read_to_end(&mut out).ok()?;
                        Some(out)
                    }
                    _ => None,
                }
            }
        }
    }

    fn json(&self, name: &str) -> Option<Value> {
        serde_json::from_slice(&self.read(name)?).ok()
    }

    /// How block `name` (namespace:path) looks.
    pub fn look(&self, name: &str) -> Look {
        let (ns, path) = name.split_once(':').unwrap_or(("minecraft", name));
        if matches!(path, "air" | "cave_air" | "void_air" | "light" | "barrier" | "structure_void") {
            return Look { solid: false, top: [0; 3], side: [0; 3] };
        }
        let fallback = Look { solid: true, top: [150, 150, 150], side: [130, 130, 130] };
        let model = self
            .json(&format!("assets/{ns}/blockstates/{path}.json"))
            .and_then(|bs| first_model(&bs));
        let (textures, parents, has_elements, fill) = match &model {
            Some(m) => self.resolve_model(m),
            None => (HashMap::new(), Vec::new(), true, 1.0),
        };
        // plants, torches, rails, signs and other small or flat things are left out of the far-away geometry
        let skip_parent = ["cross", "tinted_cross", "template_torch", "template_torch_wall", "rail_flat", "rail_raised",
            "flower_pot_cross", "crop", "stem_growth", "template_glass_pane_noside", "redstone_dust"];
        if parents.iter().any(|p| skip_parent.iter().any(|s| p.ends_with(&format!("block/{s}")))) {
            return Look { solid: false, ..fallback };
        }
        // thin things (iron bars, chains, ladders, doors, trapdoors, fences, carpets) would turn into solid walls
        // and pillars in the far-away geometry: keep only blocks that fill at least a quarter of their cube
        if has_elements && fill < 0.25 {
            return Look { solid: false, ..fallback };
        }
        if model.is_some() && !has_elements && !matches!(path, "water" | "lava" | "bubble_column") {
            return Look { solid: false, ..fallback }; // drawn by the game's entity renderer (signs, chests, banners)
        }
        let pick = |keys: &[&str]| -> Option<String> {
            for k in keys {
                if let Some(t) = textures.get(*k) {
                    return Some(t.clone());
                }
            }
            textures.values().next().cloned()
        };
        let mut top_t = pick(&["top", "end", "up", "all", "texture", "cross", "particle"]);
        let mut side_t = pick(&["side", "all", "texture", "north", "front", "particle"]);
        if top_t.is_none() && side_t.is_none() {
            let direct = format!("{ns}:block/{path}");
            top_t = Some(direct.clone());
            side_t = Some(direct);
        }
        let col = |t: &Option<String>| t.as_deref().and_then(|t| self.texture_colour(t));
        let (Some(mut top), side) = (col(&top_t).or(col(&side_t)), col(&side_t)) else { return fallback };
        let mut side = side.unwrap_or(top);
        if let Some(tint) = tint(path) {
            if path == "grass_block" {
                top = mul(top, tint); // only the top of a grass block is green
            } else {
                top = mul(top, tint);
                side = mul(side, tint);
            }
        }
        Look { solid: true, top, side }
    }

    /// Texture variables (resolved), the parent chain, whether any model in it has its own geometry, and how much of
    /// the block that geometry fills (0 to 1; the nearest model with elements wins, as in the game).
    fn resolve_model(&self, model: &str) -> (HashMap<String, String>, Vec<String>, bool, f32) {
        let mut vars: HashMap<String, String> = HashMap::new();
        let mut parents = Vec::new();
        let mut has_elements = false;
        let mut fill = 1.0f32;
        let mut cur = Some(model.to_string());
        for _ in 0..16 {
            let Some(m) = cur.take() else { break };
            let (ns, p) = m.split_once(':').unwrap_or(("minecraft", &m));
            parents.push(format!("{ns}:{p}"));
            let Some(j) = self.json(&format!("assets/{ns}/models/{p}.json")) else { break };
            if let Some(els) = j.get("elements").and_then(Value::as_array) {
                if !has_elements {
                    fill = element_fill(els);
                }
                has_elements = true;
            }
            if let Some(t) = j.get("textures").and_then(Value::as_object) {
                for (k, v) in t {
                    if let Some(s) = v.as_str() {
                        vars.entry(k.clone()).or_insert_with(|| s.to_string());
                    }
                }
            }
            cur = j.get("parent").and_then(Value::as_str).map(str::to_string);
        }
        // resolve #references
        let keys: Vec<String> = vars.keys().cloned().collect();
        let mut out = HashMap::new();
        for k in keys {
            let mut v = vars[&k].clone();
            for _ in 0..8 {
                match v.strip_prefix('#') {
                    Some(r) => match vars.get(r) {
                        Some(n) => v = n.clone(),
                        None => break,
                    },
                    None => break,
                }
            }
            if !v.starts_with('#') {
                out.insert(k, v);
            }
        }
        (out, parents, has_elements, fill)
    }

    fn texture_colour(&self, tex: &str) -> Option<[u8; 3]> {
        let (ns, p) = tex.split_once(':').unwrap_or(("minecraft", tex));
        let png = self.read(&format!("assets/{ns}/textures/{p}.png"))?;
        average_png(&png)
    }
}

fn first_model(bs: &Value) -> Option<String> {
    let pick = |v: &Value| -> Option<String> {
        let v = if let Some(a) = v.as_array() { a.first()? } else { v };
        v.get("model")?.as_str().map(str::to_string)
    };
    if let Some(vars) = bs.get("variants").and_then(Value::as_object) {
        if let Some(v) = vars.get("") {
            return pick(v);
        }
        return vars.values().next().and_then(pick);
    }
    if let Some(parts) = bs.get("multipart").and_then(Value::as_array) {
        return parts.iter().find_map(|p| p.get("apply").and_then(pick));
    }
    None
}

/// Temperate-biome tints for blocks the game colours by biome.
fn tint(path: &str) -> Option<[u8; 3]> {
    if matches!(path, "water" | "bubble_column") || path.ends_with("water_cauldron") {
        return Some([63, 118, 228]);
    }
    if path == "spruce_leaves" {
        return Some([97, 153, 97]);
    }
    if path == "birch_leaves" {
        return Some([128, 167, 85]);
    }
    if path.ends_with("_leaves") && !matches!(path, "cherry_leaves" | "azalea_leaves" | "flowering_azalea_leaves") {
        return Some([119, 171, 47]);
    }
    if matches!(path, "vine" | "lily_pad") {
        return Some([119, 171, 47]);
    }
    if matches!(path, "grass_block" | "short_grass" | "grass" | "tall_grass" | "fern" | "large_fern" | "sugar_cane") {
        return Some([145, 189, 89]);
    }
    None
}

fn mul(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|i| ((a[i] as u32 * b[i] as u32) / 255) as u8)
}

/// (name, local header offset, method, compressed size) for each entry of a zip's central directory.
fn zip_entries(d: &[u8]) -> Option<Vec<(String, usize, u16, usize)>> {
    let lo = d.len().saturating_sub(22 + 65535);
    let eocd = (lo..d.len().saturating_sub(21)).rev().find(|&i| d[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])?;
    let count = u16::from_le_bytes([d[eocd + 10], d[eocd + 11]]) as usize;
    let mut p = u32::from_le_bytes(d[eocd + 16..eocd + 20].try_into().ok()?) as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if p + 46 > d.len() || d[p..p + 4] != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }
        let method = u16::from_le_bytes([d[p + 10], d[p + 11]]);
        let csize = u32::from_le_bytes(d[p + 20..p + 24].try_into().ok()?) as usize;
        let nl = u16::from_le_bytes([d[p + 28], d[p + 29]]) as usize;
        let el = u16::from_le_bytes([d[p + 30], d[p + 31]]) as usize;
        let cl = u16::from_le_bytes([d[p + 32], d[p + 33]]) as usize;
        let off = u32::from_le_bytes(d[p + 42..p + 46].try_into().ok()?) as usize;
        let name = String::from_utf8_lossy(d.get(p + 46..p + 46 + nl)?).into_owned();
        out.push((name, off, method, csize));
        p += 46 + nl + el + cl;
    }
    Some(out)
}

/// How much of a block's cube a model's elements fill, 0 to 1 (overlaps are counted twice, so this errs high).
fn element_fill(els: &[Value]) -> f32 {
    let v3 = |e: &Value, k: &str| -> Option<[f32; 3]> {
        let a = e.get(k)?.as_array()?;
        Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
    };
    let mut vol = 0.0f32;
    for e in els {
        let (Some(f), Some(t)) = (v3(e, "from"), v3(e, "to")) else { continue };
        let d = |i: usize| (t[i].clamp(0.0, 16.0) - f[i].clamp(0.0, 16.0)).abs();
        vol += d(0) * d(1) * d(2);
    }
    (vol / 4096.0).min(1.0)
}

/// Average colour of a PNG's visible pixels (8-bit or palette images, not interlaced).
pub fn average_png(b: &[u8]) -> Option<[u8; 3]> {
    if b.len() < 8 || b[..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return None;
    }
    let (mut w, mut h, mut depth, mut ctype, mut interlace) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let mut plte: &[u8] = &[];
    let mut trns: &[u8] = &[];
    let mut idat = Vec::new();
    let mut i = 8;
    while i + 8 <= b.len() {
        let len = u32::from_be_bytes(b[i..i + 4].try_into().ok()?) as usize;
        let kind = &b[i + 4..i + 8];
        let body = b.get(i + 8..i + 8 + len)?;
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes(body[0..4].try_into().ok()?) as usize;
                h = u32::from_be_bytes(body[4..8].try_into().ok()?) as usize;
                depth = body[8];
                ctype = body[9];
                interlace = body[12];
            }
            b"PLTE" => plte = body,
            b"tRNS" => trns = body,
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len;
    }
    if interlace != 0 || w == 0 || h == 0 || w * h > 4096 * 4096 {
        return None;
    }
    let channels = match ctype {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    if depth != 8 && !(ctype == 3 && matches!(depth, 1 | 2 | 4)) {
        return None;
    }
    let bits_pp = channels * depth as usize;
    let stride = (w * bits_pp).div_ceil(8);
    let bpp = (bits_pp / 8).max(1);
    let mut raw = Vec::new();
    ZlibDecoder::new(&idat[..]).read_to_end(&mut raw).ok()?;
    if raw.len() < h * (stride + 1) {
        return None;
    }
    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    let (mut sr, mut sg, mut sb, mut sa) = (0u64, 0u64, 0u64, 0u64);
    for y in 0..h {
        let row = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let f = row[0];
        for x in 0..stride {
            let a = if x >= bpp { cur[x - bpp] } else { 0 };
            let up = prev[x];
            let c = if x >= bpp { prev[x - bpp] } else { 0 };
            let v = row[1 + x];
            cur[x] = match f {
                0 => v,
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(up),
                3 => v.wrapping_add(((a as u16 + up as u16) / 2) as u8),
                4 => {
                    let p = a as i16 + up as i16 - c as i16;
                    let (pa, pb, pc) = ((p - a as i16).abs(), (p - up as i16).abs(), (p - c as i16).abs());
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { up } else { c };
                    v.wrapping_add(pr)
                }
                _ => return None,
            };
        }
        for x in 0..w {
            let (r, g, bl, al) = match ctype {
                0 => (cur[x], cur[x], cur[x], 255),
                2 => (cur[x * 3], cur[x * 3 + 1], cur[x * 3 + 2], 255),
                4 => (cur[x * 2], cur[x * 2], cur[x * 2], cur[x * 2 + 1]),
                6 => (cur[x * 4], cur[x * 4 + 1], cur[x * 4 + 2], cur[x * 4 + 3]),
                _ => {
                    let d = depth as usize;
                    let bit = x * d;
                    let idx = ((cur[bit / 8] >> (8 - d - bit % 8)) & ((1u16 << d) - 1) as u8) as usize;
                    let p = plte.get(idx * 3..idx * 3 + 3)?;
                    (p[0], p[1], p[2], *trns.get(idx).unwrap_or(&255))
                }
            };
            let al = al as u64;
            sr += r as u64 * al;
            sg += g as u64 * al;
            sb += bl as u64 * al;
            sa += al;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    if sa == 0 {
        return None;
    }
    Some([(sr / sa) as u8, (sg / sa) as u8, (sb / sa) as u8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;

    /// A tiny RGBA PNG written in code (tests never use game files).
    pub fn png(w: usize, h: usize, px: [u8; 4]) -> Vec<u8> {
        let mut raw = Vec::new();
        for _ in 0..h {
            raw.push(0);
            for _ in 0..w {
                raw.extend_from_slice(&px);
            }
        }
        let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
        z.write_all(&raw).unwrap();
        let idat = z.finish().unwrap();
        let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
        let mut chunk = |k: &[u8], b: &[u8]| {
            out.extend_from_slice(&(b.len() as u32).to_be_bytes());
            out.extend_from_slice(k);
            out.extend_from_slice(b);
            out.extend_from_slice(&[0, 0, 0, 0]); // CRC is not checked by the reader
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(b"IHDR", &ihdr);
        chunk(b"IDAT", &idat);
        chunk(b"IEND", &[]);
        out
    }

    #[test]
    fn average_of_flat_png() {
        assert_eq!(average_png(&png(4, 4, [10, 200, 30, 255])), Some([10, 200, 30]));
        assert_eq!(average_png(&png(2, 2, [10, 200, 30, 0])), None);
    }

    #[test]
    fn looks_follow_models_from_a_folder_pack() {
        let dir = std::env::temp_dir().join(format!("chunkfetti-assets-{}", std::process::id()));
        let w = |p: &str, b: &[u8]| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        };
        w("assets/test/blockstates/brick.json", br#"{"variants":{"":{"model":"test:block/brick"}}}"#);
        w("assets/test/models/block/brick.json", br##"{"parent":"test:block/cube","textures":{"all":"test:block/brick"}}"##);
        w("assets/test/models/block/cube.json", br#"{"elements":[{"from":[0,0,0],"to":[16,16,16]}]}"#);
        w("assets/test/textures/block/brick.png", &png(2, 2, [200, 50, 40, 255]));
        w("assets/test/blockstates/flower.json", br#"{"variants":{"":{"model":"test:block/flower"}}}"#);
        w("assets/test/models/block/flower.json", br#"{"parent":"minecraft:block/cross","textures":{"cross":"test:block/brick"}}"#);
        w("assets/test/blockstates/bars.json", br#"{"multipart":[{"apply":{"model":"test:block/bars_post"}}]}"#);
        w("assets/test/models/block/bars_post.json",
          br#"{"textures":{"bars":"test:block/brick"},"elements":[{"from":[7,0,7],"to":[9,16,9]}]}"#);
        w("assets/test/blockstates/slab.json", br#"{"variants":{"type=bottom":{"model":"test:block/slab"}}}"#);
        w("assets/test/models/block/slab.json",
          br#"{"textures":{"all":"test:block/brick"},"elements":[{"from":[0,0,0],"to":[16,8,16]}]}"#);
        let mut a = Assets::default();
        a.add(&dir).unwrap();
        assert!(!a.look("test:bars").solid, "a thin post is left out");
        assert!(a.look("test:slab").solid, "a half block stays");
        let l = a.look("test:brick");
        assert!(l.solid);
        assert_eq!(l.top, [200, 50, 40]);
        assert!(!a.look("test:flower").solid);
        assert!(!a.look("minecraft:air").solid);
        std::fs::remove_dir_all(&dir).ok();
    }
}
