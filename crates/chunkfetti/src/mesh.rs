//! Greedy meshing of a tile grid into coloured quads, written as a binary glTF (.glb).
//!
//! Only faces between a solid voxel and an empty one are drawn; neighbouring faces of the same colour are merged
//! into one rectangle. Light is baked into the vertex colours (tops brightest, like the game's flat shading), and the
//! material is unlit, so the browser does no lighting work.

use crate::voxel::{side_of, top_of, Grid, TILE};

#[derive(Default)]
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub col: Vec<[u8; 4]>,
    pub idx: Vec<u32>,
}

/// Face directions: (axis, +1/-1) with the light each one gets.
const DIRS: [(usize, i32, f32); 6] = [(1, 1, 1.0), (1, -1, 0.5), (0, 1, 0.8), (0, -1, 0.8), (2, 1, 0.65), (2, -1, 0.65)];

/// Meshes a tile grid whose north-west corner is at world block (x0, z0), at `scale` blocks per voxel.
/// Coordinates are world blocks (x east, y up, z south: the game's own axes).
pub fn mesh(g: &Grid, x0: i64, z0: i64, scale: i32) -> Mesh {
    let mut m = Mesh::default();
    let ny = g.ny;
    let solid = |x: i32, y: i32, z: i32| -> u32 {
        if y < 0 || y >= ny as i32 {
            return 0;
        }
        g.get(x as usize, y as usize, z as usize)
    };
    let ox = x0 as f32;
    let oz = z0 as f32;
    let oy = g.y0 as f32 * scale as f32;
    let s = scale as f32;
    for &(axis, dir, light) in &DIRS {
        // the slice runs along `axis`; (u, v) are the other two axes
        let (ua, va) = match axis {
            0 => (2, 1),
            1 => (0, 2),
            _ => (0, 1),
        };
        let len = |a: usize| if a == 1 { ny } else { TILE };
        let (nu, nv, ns) = (len(ua), len(va), len(axis));
        let mut mask = vec![0u32; nu * nv];
        for k in 0..ns {
            // fill the mask of visible faces in this slice
            let mut any = false;
            for v in 0..nv {
                for u in 0..nu {
                    let mut p = [0i32; 3];
                    p[axis] = k as i32;
                    p[ua] = u as i32;
                    p[va] = v as i32;
                    // grid x/z are offset by the border
                    let (x, y, z) = (p[0] + 1, p[1], p[2] + 1);
                    let here = solid(x, y, z);
                    let mut q = [x, y, z];
                    q[axis] += dir;
                    let c = if here != 0 && solid(q[0], q[1], q[2]) == 0 {
                        let rgb = if axis == 1 && dir == 1 { top_of(here) } else { side_of(here) };
                        let l = |c: u8| ((c as f32 * light).min(255.0)) as u32;
                        any = true;
                        0xff00_0000 | (l(rgb[0]) << 16) | (l(rgb[1]) << 8) | l(rgb[2])
                    } else {
                        0
                    };
                    mask[v * nu + u] = c;
                }
            }
            if !any {
                continue;
            }
            // greedy: grow rectangles of one colour
            for v in 0..nv {
                let mut u = 0;
                while u < nu {
                    let c = mask[v * nu + u];
                    if c == 0 {
                        u += 1;
                        continue;
                    }
                    let mut w = 1;
                    while u + w < nu && mask[v * nu + u + w] == c {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while v + h < nv {
                        for i in 0..w {
                            if mask[(v + h) * nu + u + i] != c {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }
                    for j in 0..h {
                        for i in 0..w {
                            mask[(v + j) * nu + u + i] = 0;
                        }
                    }
                    // the quad's corners in voxel units
                    let plane = k as f32 + if dir > 0 { 1.0 } else { 0.0 };
                    let corner = |du: f32, dv: f32| -> [f32; 3] {
                        let mut p = [0f32; 3];
                        p[axis] = plane;
                        p[ua] = u as f32 + du;
                        p[va] = v as f32 + dv;
                        [ox + p[0] * s, oy + p[1] * s, oz + p[2] * s]
                    };
                    let (wf, hf) = (w as f32, h as f32);
                    let base = m.pos.len() as u32;
                    m.pos.extend_from_slice(&[corner(0.0, 0.0), corner(wf, 0.0), corner(wf, hf), corner(0.0, hf)]);
                    let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
                    m.col.extend_from_slice(&[rgba; 4]);
                    // wind counter-clockwise as seen from outside: the quad's normal must point along `dir`
                    let (a, b, c3) = (m.pos[base as usize], m.pos[base as usize + 1], m.pos[base as usize + 2]);
                    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let e2 = [c3[0] - a[0], c3[1] - a[1], c3[2] - a[2]];
                    let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                    if n[axis] * dir as f32 > 0.0 {
                        m.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                    } else {
                        m.idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
                    }
                    u += w;
                }
            }
        }
    }
    m
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    /// Binary glTF 2.0: positions, vertex colours, indices, one unlit material.
    pub fn glb(&self) -> Vec<u8> {
        let nv = self.pos.len();
        let mut bin = Vec::new();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
                bin.extend_from_slice(&p[k].to_le_bytes());
            }
        }
        let col_off = bin.len();
        for c in &self.col {
            bin.extend_from_slice(c);
        }
        let idx_off = bin.len();
        let wide = nv > 65535;
        for &i in &self.idx {
            if wide {
                bin.extend_from_slice(&i.to_le_bytes());
            } else {
                bin.extend_from_slice(&(i as u16).to_le_bytes());
            }
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let json = serde_json::json!({
            "asset": {"version": "2.0", "generator": "chunkfetti"},
            "extensionsUsed": ["KHR_materials_unlit"],
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0, "roughnessFactor": 1},
                           "extensions": {"KHR_materials_unlit": {}}}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "COLOR_0": 1}, "indices": 2, "material": 0}]}],
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": col_off, "target": 34962},
                {"buffer": 0, "byteOffset": col_off, "byteLength": idx_off - col_off, "target": 34962},
                {"buffer": 0, "byteOffset": idx_off, "byteLength": self.idx.len() * if wide { 4 } else { 2 }, "target": 34963}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": nv, "type": "VEC3", "min": lo, "max": hi},
                {"bufferView": 1, "componentType": 5121, "normalized": true, "count": nv, "type": "VEC4"},
                {"bufferView": 2, "componentType": if wide { 5125 } else { 5123 }, "count": self.idx.len(), "type": "SCALAR"}
            ]
        });
        let mut js = serde_json::to_vec(&json).unwrap();
        while js.len() % 4 != 0 {
            js.push(b' ');
        }
        let total = 12 + 8 + js.len() + 8 + bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(js.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&js);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::pack;

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }

    #[test]
    fn one_block_has_six_outward_faces() {
        let mut g = Grid::new(0, 3);
        g.set(5, 1, 7, pack([1, 2, 3], [4, 5, 6]));
        let m = mesh(&g, 0, 0, 1);
        assert_eq!(m.idx.len(), 36);
        let centre = [4.5f32, 1.5, 6.5];
        for t in m.idx.chunks(3) {
            let (a, b, c) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
            let n = cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let out = [a[0] - centre[0], a[1] - centre[1], a[2] - centre[2]];
            let dot = n[0] * out[0] + n[1] * out[1] + n[2] * out[2];
            assert!(dot > 0.0, "triangle faces inward");
        }
    }

    #[test]
    fn a_flat_floor_merges_into_few_quads() {
        let mut g = Grid::new(0, 1);
        for z in 1..=TILE {
            for x in 1..=TILE {
                g.set(x, 0, z, pack([90, 90, 90], [80, 80, 80]));
            }
        }
        let m = mesh(&g, 0, 0, 1);
        // top + bottom + four sides, each one rectangle
        assert_eq!(m.idx.len(), 6 * 6);
        let glb = m.glb();
        assert_eq!(&glb[..4], b"glTF");
    }
}
