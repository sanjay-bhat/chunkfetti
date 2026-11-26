//! Minimal reader for the game's NBT format (big-endian, uncompressed buffer).

#[derive(Debug, Clone)]
#[allow(dead_code)] // every tag kind is parsed; only some are read
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[Tag]> {
        match self {
            Tag::List(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Tag::Byte(v) => Some(v as i64),
            Tag::Short(v) => Some(v as i64),
            Tag::Int(v) => Some(v as i64),
            Tag::Long(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_longs(&self) -> Option<&[i64]> {
        match self {
            Tag::LongArray(v) => Some(v),
            _ => None,
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

type R<T> = Result<T, String>;

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if self.i + n > self.b.len() {
            return Err("NBT ended early".into());
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> R<i16> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> R<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> R<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn len(&mut self) -> R<usize> {
        let n = self.i32()?;
        if n < 0 {
            return Ok(0);
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> R<String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, id: u8, depth: u32) -> R<Tag> {
        if depth > 512 {
            return Err("NBT nested too deep".into());
        }
        Ok(match id {
            0 => Tag::End,
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.to_vec())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let et = self.u8()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    v.push(self.payload(et, depth + 1)?);
                }
                Tag::List(v)
            }
            10 => {
                let mut items = Vec::new();
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        break;
                    }
                    let k = self.string()?;
                    items.push((k, self.payload(t, depth + 1)?));
                }
                Tag::Compound(items)
            }
            11 => {
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(1 << 20));
                for _ in 0..n {
                    v.push(self.i32()?);
                }
                Tag::IntArray(v)
            }
            12 => {
                let n = self.len()?;
                let raw = self.take(n.checked_mul(8).ok_or("bad length")?)?;
                Tag::LongArray(raw.chunks_exact(8).map(|c| i64::from_be_bytes(c.try_into().unwrap())).collect())
            }
            t => return Err(format!("unknown NBT tag {t}")),
        })
    }
}

/// Parses a whole NBT document (root compound with its name).
pub fn parse(b: &[u8]) -> R<Tag> {
    let mut r = Reader { b, i: 0 };
    let id = r.u8()?;
    if id != 10 {
        return Err("NBT root is not a compound".into());
    }
    let _name = r.string()?;
    r.payload(10, 0)
}

/// Writes NBT (used by tests to make tiny worlds).
#[cfg(test)]
pub fn write(tag: &Tag, out: &mut Vec<u8>) {
    out.push(10);
    out.extend_from_slice(&0u16.to_be_bytes());
    write_payload(tag, out);
}

#[cfg(test)]
fn id(t: &Tag) -> u8 {
    match t {
        Tag::End => 0,
        Tag::Byte(_) => 1,
        Tag::Short(_) => 2,
        Tag::Int(_) => 3,
        Tag::Long(_) => 4,
        Tag::Float(_) => 5,
        Tag::Double(_) => 6,
        Tag::ByteArray(_) => 7,
        Tag::String(_) => 8,
        Tag::List(_) => 9,
        Tag::Compound(_) => 10,
        Tag::IntArray(_) => 11,
        Tag::LongArray(_) => 12,
    }
}

#[cfg(test)]
fn write_payload(t: &Tag, o: &mut Vec<u8>) {
    match t {
        Tag::End => {}
        Tag::Byte(v) => o.push(*v as u8),
        Tag::Short(v) => o.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => o.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => o.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => o.extend_from_slice(&v.to_be_bytes()),
        Tag::Double(v) => o.extend_from_slice(&v.to_be_bytes()),
        Tag::ByteArray(v) => {
            o.extend_from_slice(&(v.len() as i32).to_be_bytes());
            o.extend_from_slice(v);
        }
        Tag::String(s) => {
            o.extend_from_slice(&(s.len() as u16).to_be_bytes());
            o.extend_from_slice(s.as_bytes());
        }
        Tag::List(v) => {
            o.push(v.first().map(id).unwrap_or(0));
            o.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                write_payload(x, o);
            }
        }
        Tag::Compound(items) => {
            for (k, v) in items {
                o.push(id(v));
                o.extend_from_slice(&(k.len() as u16).to_be_bytes());
                o.extend_from_slice(k.as_bytes());
                write_payload(v, o);
            }
            o.push(0);
        }
        Tag::IntArray(v) => {
            o.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                o.extend_from_slice(&x.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            o.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                o.extend_from_slice(&x.to_be_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let t = Tag::Compound(vec![
            ("a".into(), Tag::Int(7)),
            ("s".into(), Tag::String("stone".into())),
            ("l".into(), Tag::LongArray(vec![1, -2, 3])),
            ("c".into(), Tag::List(vec![Tag::Compound(vec![("Y".into(), Tag::Byte(-4))])])),
        ]);
        let mut b = Vec::new();
        write(&t, &mut b);
        let p = parse(&b).unwrap();
        assert_eq!(p.get("a").unwrap().as_i64(), Some(7));
        assert_eq!(p.get("s").unwrap().as_str(), Some("stone"));
        assert_eq!(p.get("l").unwrap().as_longs(), Some(&[1i64, -2, 3][..]));
        assert_eq!(p.get("c").unwrap().as_list().unwrap()[0].get("Y").unwrap().as_i64(), Some(-4));
    }
}
