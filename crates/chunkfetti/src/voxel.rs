//! Voxel grids for one tile at one level of detail.
//!
//! Every tile is TILE x TILE voxels across (plus a one-voxel border borrowed from its neighbours, so faces on the
//! tile edge are culled correctly) and as tall as its contents. At the finest level a voxel is one block; each level
//! up halves the resolution: a voxel there stands for 2x2x2 voxels of the level below.
//!
//! A voxel is a u32: 0 = empty, otherwise the top colour (RGB565) in the high half and the side colour in the low half.

use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use std::io::{Read, Write};

pub const TILE: usize = 64;
pub const D: usize = TILE + 2; // with the border
pub const HALF: usize = TILE / 2;

pub fn rgb565(c: [u8; 3]) -> u16 {
    ((c[0] as u16 >> 3) << 11) | ((c[1] as u16 >> 2) << 5) | (c[2] as u16 >> 3)
}

pub fn from565(v: u16) -> [u8; 3] {
    let r = ((v >> 11) & 31) as u32;
    let g = ((v >> 5) & 63) as u32;
    let b = (v & 31) as u32;
    [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
}

/// Packs a solid voxel; never returns 0 (that means empty).
pub fn pack(top: [u8; 3], side: [u8; 3]) -> u32 {
    ((rgb565(top) as u32) << 16) | (rgb565(side) as u32) | 0x0001_0000_u32 * ((top == [0; 3] && side == [0; 3]) as u32)
}

pub fn top_of(v: u32) -> [u8; 3] {
    from565((v >> 16) as u16)
}

pub fn side_of(v: u32) -> [u8; 3] {
    from565(v as u16)
}

/// A tile's grid: D x D columns (index 0 and D-1 are the border), `ny` voxels tall starting at voxel-y `y0`.
pub struct Grid {
    pub y0: i32,
    pub ny: usize,
    pub v: Vec<u32>,
}

impl Grid {
    pub fn new(y0: i32, ny: usize) -> Grid {
        Grid { y0, ny, v: vec![0; D * D * ny] }
    }
    #[inline]
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (y * D + z) * D + x
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        self.v[self.idx(x, y, z)]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, val: u32) {
        let i = self.idx(x, y, z);
        self.v[i] = val;
    }

    /// Lowest and highest occupied voxel-y inside the tile (border excluded).
    pub fn occupied(&self) -> Option<(i32, i32)> {
        let mut lo = None;
        let mut hi = None;
        for y in 0..self.ny {
            let any = (1..=TILE).any(|z| (1..=TILE).any(|x| self.get(x, y, z) != 0));
            if any {
                lo.get_or_insert(y);
                hi = Some(y);
            }
        }
        Some((self.y0 + lo? as i32, self.y0 + hi? as i32))
    }

    /// The tile's interior at half resolution: what its parent tile will use.
    pub fn half(&self) -> Half {
        let y0 = self.y0.div_euclid(2);
        let top = self.y0 + self.ny as i32; // exclusive, in this grid's voxel units
        let ny = ((top + 1).div_euclid(2) - y0).max(0) as usize;
        let mut cols = vec![Vec::new(); HALF * HALF];
        for hz in 0..HALF {
            for hx in 0..HALF {
                let mut col = Vec::with_capacity(ny);
                for hy in 0..ny {
                    let mut n = 0u32;
                    let mut s = [0u32; 6];
                    for dy in 0..2 {
                        let y = (2 * (y0 + hy as i32) + dy) - self.y0;
                        if y < 0 || y >= self.ny as i32 {
                            continue;
                        }
                        for dz in 0..2 {
                            for dx in 0..2 {
                                let v = self.get(1 + 2 * hx + dx, y as usize, 1 + 2 * hz + dz);
                                if v != 0 {
                                    n += 1;
                                    let (t, sd) = (top_of(v), side_of(v));
                                    for k in 0..3 {
                                        s[k] += t[k] as u32;
                                        s[3 + k] += sd[k] as u32;
                                    }
                                }
                            }
                        }
                    }
                    // a coarse voxel is solid when at least half of its eight are: floors and walls one block
                    // thick survive, and so do tunnels and rooms underground (a looser rule fills them in)
                    col.push(if n >= 4 {
                        let t = [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8];
                        let sd = [(s[3] / n) as u8, (s[4] / n) as u8, (s[5] / n) as u8];
                        pack(t, sd)
                    } else {
                        0
                    });
                }
                cols[hz * HALF + hx] = rle(&col);
            }
        }
        Half { y0, ny, cols }
    }
}

/// A child tile's contribution to its parent: HALF x HALF columns, run-length coded along y.
pub struct Half {
    pub y0: i32,
    pub ny: usize,
    pub cols: Vec<Vec<(u16, u32)>>,
}

fn rle(col: &[u32]) -> Vec<(u16, u32)> {
    let mut out: Vec<(u16, u32)> = Vec::new();
    for &v in col {
        match out.last_mut() {
            Some((n, last)) if *last == v && *n < u16::MAX => *n += 1,
            _ => out.push((1, v)),
        }
    }
    out
}

impl Half {
    pub fn is_empty(&self) -> bool {
        self.cols.iter().all(|c| c.iter().all(|&(_, v)| v == 0))
    }

    /// Column (hx, hz) as voxels from y0 upwards.
    pub fn column(&self, hx: usize, hz: usize) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.ny);
        for &(n, v) in &self.cols[hz * HALF + hx] {
            out.extend(std::iter::repeat(v).take(n as usize));
        }
        out
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&self.y0.to_le_bytes());
        raw.extend_from_slice(&(self.ny as u32).to_le_bytes());
        for c in &self.cols {
            raw.extend_from_slice(&(c.len() as u32).to_le_bytes());
            for &(n, v) in c {
                raw.extend_from_slice(&n.to_le_bytes());
                raw.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
        z.write_all(&raw).unwrap();
        z.finish().unwrap()
    }

    pub fn from_bytes(b: &[u8]) -> Option<Half> {
        let mut raw = Vec::new();
        ZlibDecoder::new(b).read_to_end(&mut raw).ok()?;
        let rd = |i: usize, n: usize| raw.get(i..i + n);
        let y0 = i32::from_le_bytes(rd(0, 4)?.try_into().ok()?);
        let ny = u32::from_le_bytes(rd(4, 4)?.try_into().ok()?) as usize;
        let mut p = 8;
        let mut cols = Vec::with_capacity(HALF * HALF);
        for _ in 0..HALF * HALF {
            let k = u32::from_le_bytes(rd(p, 4)?.try_into().ok()?) as usize;
            p += 4;
            let mut c = Vec::with_capacity(k);
            for _ in 0..k {
                let n = u16::from_le_bytes(rd(p, 2)?.try_into().ok()?);
                let v = u32::from_le_bytes(rd(p + 2, 4)?.try_into().ok()?);
                c.push((n, v));
                p += 6;
            }
            cols.push(c);
        }
        Some(Half { y0, ny, cols })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip_closely() {
        let v = pack([200, 100, 50], [10, 20, 30]);
        assert_ne!(v, 0);
        let t = top_of(v);
        assert!((t[0] as i32 - 200).abs() < 9 && (t[1] as i32 - 100).abs() < 5);
        assert_ne!(pack([0, 0, 0], [0, 0, 0]), 0);
    }

    #[test]
    fn half_keeps_a_floor_and_round_trips() {
        let mut g = Grid::new(-4, 8);
        for z in 1..=TILE {
            for x in 1..=TILE {
                g.set(x, 0, z, pack([90, 90, 90], [80, 80, 80]));
                g.set(x, 1, z, pack([90, 90, 90], [80, 80, 80]));
            }
        }
        let h = g.half();
        assert_eq!(h.y0, -2);
        let c = h.column(3, 5);
        assert_ne!(c[0], 0);
        assert!(c[1..].iter().all(|&v| v == 0));
        let back = Half::from_bytes(&h.to_bytes()).unwrap();
        assert_eq!(back.column(3, 5), c);
        assert!(!back.is_empty());
    }
}
