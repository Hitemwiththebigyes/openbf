//! BF2 cloth meshes (`CLTH`), read the way B2 FUN_001d9ca8 reads them
//! (`analysis/re-cloth.md` 1 [O]). BF1 has none.
//!
//! Children: `INFO` the owner model's name, `NAME` the cloth's name (the key a
//! `cloth` class and a gmod's INFO list name it by), `PRNT` the bone the cloth
//! frame hangs on, `XFRM` 12 f32 (rows r0 r1 r2 t, row vectors: the cloth
//! frame in the PRNT bone's frame), then two BYTE STREAMS whose strings are
//! NUL-terminated and NOT padded:
//!
//! - `DATA`: texture name; u32 N; N x vec3 positions (the cloth frame); N x
//!   vec2 UVs; u32 F (vertices 0..F-1 are pinned - no index list); u32 W (0 or
//!   F) + W bone names (pinned vertex i's bone; W = 0 leaves the pins in the
//!   cloth frame, the CTF flags); u32 T + T x 3 u32 indices; three lists, each
//!   u32 count + count x (u32 a, u32 b) (stretch, cross, bend - the order
//!   DATA stores them; re-cloth 3 for which class bit gates which).
//! - `COLL`: u32 K; K x (bone name, u32 type 0 sphere / 1 cylinder / 2 box,
//!   3 f32 params, 12 f32 rows r0 r1 r2 t).
//!
//! All 58 disc chunks end exactly on their DATA and COLL ends (the test in
//! `tests/suite/cloth_disc.rs`).

use crate::{le, Chunk, UcfbError};

#[derive(Debug, Clone, PartialEq)]
pub struct ClothShape {
    pub bone: String,
    /// 0 sphere, 1 cylinder, 2 box (re-cloth 4.5).
    pub kind: u32,
    pub params: [f32; 3],
    /// Rows r0 r1 r2 t (row vectors), the shape in its bone's frame.
    pub rows: [[f32; 3]; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cloth {
    pub name: String,
    /// The model the chunk was munged for (`INFO`).
    pub owner: String,
    /// The bone the cloth frame hangs on (`PRNT`).
    pub parent_bone: String,
    /// The cloth frame in the PRNT bone's frame: rows r0 r1 r2 t.
    pub xfrm: [[f32; 3]; 4],
    pub texture: String,
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Vertices `0..pinned` are pinned.
    pub pinned: usize,
    /// Pinned vertex i's bone (`pinned` names, or none: the pins stay in the
    /// cloth frame).
    pub pin_bones: Vec<String>,
    pub triangles: Vec<[u32; 3]>,
    pub stretch: Vec<[u32; 2]>,
    pub cross: Vec<[u32; 2]>,
    pub bend: Vec<[u32; 2]>,
    pub shapes: Vec<ClothShape>,
}

/// A byte-stream reader: every read is bounds-checked against the payload.
struct Stream<'a> {
    b: &'a [u8],
    at: usize,
    chunk: usize,
}

impl<'a> Stream<'a> {
    fn bad(&self) -> UcfbError {
        UcfbError::Truncated(self.chunk)
    }
    fn u32(&mut self) -> Result<u32, UcfbError> {
        if self.at + 4 > self.b.len() {
            return Err(self.bad());
        }
        let v = le::u32_at(self.b, self.at);
        self.at += 4;
        Ok(v)
    }
    fn f32(&mut self) -> Result<f32, UcfbError> {
        self.u32().map(f32::from_bits)
    }
    fn cstr(&mut self) -> Result<String, UcfbError> {
        let rest = self.b.get(self.at..).ok_or_else(|| self.bad())?;
        let end = rest
            .iter()
            .position(|&c| c == 0)
            .ok_or_else(|| self.bad())?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.at += end + 1;
        Ok(s)
    }
    /// A count about to size `count * each` bytes: refused when the payload
    /// cannot hold them (a corrupt count must not allocate).
    fn count(&mut self, each: usize) -> Result<usize, UcfbError> {
        let n = self.u32()? as usize;
        if n.saturating_mul(each) > self.b.len() - self.at {
            return Err(self.bad());
        }
        Ok(n)
    }
    fn pairs(&mut self) -> Result<Vec<[u32; 2]>, UcfbError> {
        let n = self.count(8)?;
        (0..n).map(|_| Ok([self.u32()?, self.u32()?])).collect()
    }
    fn rows(&mut self) -> Result<[[f32; 3]; 4], UcfbError> {
        let mut m = [[0.0; 3]; 4];
        for row in &mut m {
            for v in row.iter_mut() {
                *v = self.f32()?;
            }
        }
        Ok(m)
    }
    fn done(&self) -> Result<(), UcfbError> {
        if self.at == self.b.len() {
            Ok(())
        } else {
            Err(UcfbError::Malformed(format!(
                "CLTH @{:#x}: {} bytes left over",
                self.chunk,
                self.b.len() - self.at
            )))
        }
    }
}

impl Cloth {
    pub fn parse(clth: &Chunk<'_>) -> Result<Cloth, UcfbError> {
        let bad = || UcfbError::Truncated(clth.offset);
        let name = clth.child_string(b"NAME").ok_or_else(bad)?;
        let owner = clth.child_string(b"INFO").unwrap_or_default();
        let parent_bone = clth.child_string(b"PRNT").ok_or_else(bad)?;
        let x = clth.child(b"XFRM").ok_or_else(bad)?.payload;
        let mut xs = Stream {
            b: x,
            at: 0,
            chunk: clth.offset,
        };
        let xfrm = xs.rows()?;

        let data = clth.child(b"DATA").ok_or_else(bad)?.payload;
        let mut s = Stream {
            b: data,
            at: 0,
            chunk: clth.offset,
        };
        let texture = s.cstr()?;
        let n = s.count(20)?;
        let positions = (0..n)
            .map(|_| Ok([s.f32()?, s.f32()?, s.f32()?]))
            .collect::<Result<Vec<_>, UcfbError>>()?;
        let uvs = (0..n)
            .map(|_| Ok([s.f32()?, s.f32()?]))
            .collect::<Result<Vec<_>, UcfbError>>()?;
        let pinned = s.u32()? as usize;
        let w = s.count(1)?;
        let pin_bones = (0..w).map(|_| s.cstr()).collect::<Result<Vec<_>, _>>()?;
        let t = s.count(12)?;
        let triangles = (0..t)
            .map(|_| Ok([s.u32()?, s.u32()?, s.u32()?]))
            .collect::<Result<Vec<_>, UcfbError>>()?;
        let stretch = s.pairs()?;
        let cross = s.pairs()?;
        let bend = s.pairs()?;
        s.done()?;
        if pinned > n || !(w == 0 || w == pinned) {
            return Err(UcfbError::Malformed(format!(
                "CLTH {name}: {pinned} pinned, {w} pin bones of {n} vertices"
            )));
        }
        let all = triangles.iter().flatten();
        let lists = stretch.iter().chain(&cross).chain(&bend).flatten();
        if all.chain(lists).any(|&i| i as usize >= n) {
            return Err(UcfbError::Malformed(format!("CLTH {name}: index past {n}")));
        }

        let mut shapes = Vec::new();
        if let Some(c) = clth.child(b"COLL") {
            let mut s = Stream {
                b: c.payload,
                at: 0,
                chunk: clth.offset,
            };
            let k = s.count(64)?;
            for _ in 0..k {
                let bone = s.cstr()?;
                let kind = s.u32()?;
                let params = [s.f32()?, s.f32()?, s.f32()?];
                let rows = s.rows()?;
                shapes.push(ClothShape {
                    bone,
                    kind,
                    params,
                    rows,
                });
            }
            s.done()?;
        }
        Ok(Cloth {
            name,
            owner,
            parent_bone,
            xfrm,
            texture,
            positions,
            uvs,
            pinned,
            pin_bones,
            triangles,
            stretch,
            cross,
            bend,
            shapes,
        })
    }
}

/// Every `CLTH` of an opened LVL, the FIRST of each name only: the PS2 keeps
/// the first chunk per NAME and frees a later one (IMP.LVL carries the skirt
/// once per `lvl_`; B2:86539-86562, re-cloth 1).
pub fn find_cloths(inner: &[u8]) -> Result<Vec<Cloth>, UcfbError> {
    let mut out: Vec<Cloth> = Vec::new();
    for c in crate::content_chunks(inner)? {
        if &c.tag != b"CLTH" {
            continue;
        }
        let cloth = Cloth::parse(&c)?;
        if !out.iter().any(|o| o.name.eq_ignore_ascii_case(&cloth.name)) {
            out.push(cloth);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_chunk as chunk;

    fn u(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    fn f(v: f32) -> [u8; 4] {
        v.to_le_bytes()
    }

    /// One triangle, vertex 0 pinned to `bone_a`; strings unpadded in DATA
    /// (a 5-byte texture name leaves the counts unaligned) and COLL.
    fn sample(extra: &[u8]) -> Vec<u8> {
        let mut data = b"tex1\0".to_vec();
        data.extend(u(3));
        for p in [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0f32]] {
            p.iter().for_each(|&x| data.extend(f(x)));
        }
        for t in [[0.0, 0.5], [1.0, 0.5], [0.5, 1.0f32]] {
            t.iter().for_each(|&x| data.extend(f(x)));
        }
        data.extend(u(1));
        data.extend(u(1));
        data.extend(b"bone_a\0");
        data.extend(u(1));
        [0, 1, 2].iter().for_each(|&i| data.extend(u(i)));
        data.extend(u(1));
        data.extend(u(0));
        data.extend(u(1));
        data.extend(u(0));
        data.extend(u(0));
        data.extend(extra);
        let mut coll = u(1).to_vec();
        coll.extend(b"bone_b\0");
        coll.extend(u(1));
        for i in 0..15 {
            coll.extend(f(i as f32));
        }
        let mut xf = Vec::new();
        for i in 0..12 {
            xf.extend(f(i as f32));
        }
        let mut body = chunk(b"INFO", b"model\0");
        body.extend(chunk(b"NAME", b"model_cape\0"));
        body.extend(chunk(b"PRNT", b"bone_root\0"));
        body.extend(chunk(b"XFRM", &xf));
        body.extend(chunk(b"DATA", &data));
        body.extend(chunk(b"COLL", &coll));
        chunk(b"CLTH", &body)
    }

    fn parse(buf: &[u8]) -> Result<Cloth, UcfbError> {
        let c = crate::chunks(buf).next().unwrap().unwrap();
        Cloth::parse(&c)
    }

    #[test]
    fn reads_the_unpadded_streams() {
        let c = parse(&sample(&[])).unwrap();
        assert_eq!((c.name.as_str(), c.owner.as_str()), ("model_cape", "model"));
        assert_eq!(c.parent_bone, "bone_root");
        assert_eq!(c.xfrm[3], [9.0, 10.0, 11.0]);
        assert_eq!(c.texture, "tex1");
        assert_eq!(c.positions[2], [0.0, 0.0, 1.0]);
        assert_eq!(c.uvs[1], [1.0, 0.5]);
        assert_eq!(
            (c.pinned, c.pin_bones.clone()),
            (1, vec!["bone_a".to_string()])
        );
        assert_eq!(c.triangles, vec![[0, 1, 2]]);
        assert_eq!(
            (c.stretch.clone(), c.cross.len(), c.bend.len()),
            (vec![[0, 1]], 0, 0)
        );
        let s = &c.shapes[0];
        assert_eq!(
            (s.bone.as_str(), s.kind, s.params),
            ("bone_b", 1, [0.0, 1.0, 2.0])
        );
        assert_eq!(s.rows[3], [12.0, 13.0, 14.0]);
    }

    /// A stray byte after the lists is a misread layout, not slack.
    #[test]
    fn refuses_leftover_bytes() {
        assert!(parse(&sample(&[0])).is_err());
    }

    /// The no-panic contract: every word of the chunk set to 0xFFFFFFFF (a huge
    /// count, an index past N) and every byte flipped parses to a `Result`.
    #[test]
    fn hostile_words_and_bytes_never_panic() {
        fn try_parse(b: &[u8]) -> usize {
            match crate::chunks(b).next() {
                Some(Ok(c)) => {
                    let _ = Cloth::parse(&c);
                    1
                }
                _ => 0,
            }
        }
        let good = sample(&[]);
        let mut cases = 0;
        for at in (0..good.len()).step_by(4) {
            let mut b = good.clone();
            b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            cases += try_parse(&b);
        }
        for at in 0..good.len() {
            let mut b = good.clone();
            b[at] ^= 0xff;
            cases += try_parse(&b);
        }
        assert!(cases > good.len(), "{cases} cases");
    }
}
