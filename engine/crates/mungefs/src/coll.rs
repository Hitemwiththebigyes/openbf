//! `coll` (collision meshes) and `prim` (collision primitives) chunk parsing —
//! the geometry the sim walks on, in the same model space as the render `modl`.
//!
//! Measured on the raw discs by the lead: RE_LOG 2026-09-01 coll/prim/CUTR
//! measurement (Fable session 7). Outer layout also listed in
//! analysis/a2-bake-formats.md §coll.
//!
//! `coll` children in order: `NAME` (the GEOMETRY name, e.g.
//! `kam_bldg_bunker1` — matches the `entc GeometryName` / gmod name, NOT a LOD
//! `modl` name), `NODE` (the collision node name in the source msh, e.g.
//! `bunker_export` — informational), `INFO` (40 B), `POSI`, `TREE`.
//! - `INFO` = `u32 vertexCount, u32 nodeCount, u32 leafCount, u32
//!   leafIndexTotal, f32 bboxMin[3], f32 bboxMax[3]`. Verified on 48/48 chunks
//!   over BF1 KAM1, BF2 KAM1, BF1 BES1: vertexCount == POSI len/12, nodeCount
//!   == number of `NODE` children of TREE, leafCount == number of `LEAF`
//!   children, leafIndexTotal == Σ over leaves of the leaf's index count.
//! - `POSI` = vertexCount × `f32[3]`, MODEL SPACE (the coll bbox equals the
//!   baked GLB's POSITION bbox on every model checked: kam_bldg_bunker1,
//!   kam_bldg_tower1, kam_bldg_mushplatform1, bes1_bldg_platform_med,
//!   kas2_bldg_pier ...).
//! - `TREE` = a flat sequence of `NODE` (6 B, BSP split data — NOT needed,
//!   skipped) and `LEAF` chunks. `LEAF` payload = `u8 n` `u8[6] unknown`
//!   `u16 index[n]`; size is always 7 + 2n. The n indices are one CONVEX PLANAR
//!   POLYGON in perimeter order, triangulated here as a fan
//!   (idx[0], idx[i], idx[i+1]). Measured: 0 of 5,000+ four-index leaves are
//!   zig-zag ordered (bow-tie test), all indices < vertexCount. NOT every
//!   leaf is convex (card STARC): 73 of the discs' 274,308 are not, and where
//!   such a leaf's fans overlap its neighbours' they are a signed fan of a
//!   face with a hole — [`crate::coll_fan`] repairs those. The 6 unknown
//!   bytes are NOT a normal (every byte-triple tested against the geometric
//!   normal: ≤25 % agreement) — they are deliberately kept out of this API.
//! - Winding is NOT reliable for facing (closed platforms have as many
//!   down-facing as up-facing polygons), so every consumer test is two-sided.
//!
//! `prim` children: `INFO` = cstring model/geometry name + `u32 count`; then
//! `count` repetitions of `NAME` (primitive name), `MASK` (BF2 only, 1 byte —
//! the layer mask, see [`Primitive::mask`]), `PRNT` (parent node name),
//! `XFRM` (48 B: 3×3 f32 rotation row-major + f32[3] translation — same layout
//! as wrld/skel XFRM), `DATA` (16 B: `u32 kind, f32 a, f32 b, f32 c`).
//! kind 0 = SPHERE (a = radius), kind 2 = CYLINDER (a = radius, b = FULL
//! height, axis = the primitive's local Y, centred on the XFRM translation),
//! kind 4 = BOX ((a, b, c) = HALF extents). Measured conventions:
//! `kam_bldg_tower2 p_collision` cyl (6.26, 64.0) at y = 32.17 spans
//! 0.17..64.17 (full height); `kam_prop_crate p_collision` box half 1.27 at
//! y = 1.29 sits on the ground; `kam_bldg_mushplatform1 p_collision_floor` =
//! cyl (102.5, 8.0) at (0, −4, 0) — the Kamino platform floor, whose top is the
//! platform's y = 0. Any other kind is [`PrimKind::Unknown`] and carries no
//! dimensions.
//!
//! `XFRM` is treated as MODEL space — PROVISIONAL: the `PRNT` node's own
//! transform is assumed to be identity. Every primitive checked sits inside its
//! model's render bbox, which is what makes the assumption plausible, but the
//! msh node hierarchy itself is not decoded.

use crate::{le, Chunk, UcfbError};

/// One `coll` chunk: model-space vertices plus the polygons of its BSP
/// leaves, in leaf order (all but 73 on the discs are convex - module header).
#[derive(Debug, Clone)]
pub struct CollisionMesh {
    /// The GEOMETRY name, as authored.
    pub name: String,
    /// The source-msh collision node name (informational).
    pub node: String,
    /// INFO's model-space bounding box (min, max).
    pub bbox: ([f32; 3], [f32; 3]),
    pub vertices: Vec<[f32; 3]>,
    /// One planar polygon per `LEAF`, in perimeter order, leaf order. Not
    /// always convex: [`CollisionMesh::triangles`] is the fan with the repairs.
    pub polygons: Vec<Vec<u16>>,
}

impl CollisionMesh {
    pub fn parse(coll: &Chunk<'_>) -> Result<CollisionMesh, UcfbError> {
        let bad = || UcfbError::Truncated(coll.offset);
        let name = coll.child_string(b"NAME").ok_or_else(bad)?;
        // NODE is informational and present on every shipped chunk; a missing
        // one is not worth failing a bake over.
        let node = coll.child_string(b"NODE").unwrap_or_default();
        let info = coll.child(b"INFO").ok_or_else(bad)?.payload;
        if info.len() < 40 {
            return Err(bad());
        }
        let u = |o: usize| le::u32_at(info, o);
        let f = |o: usize| le::f32_at(info, o);
        let (vertex_count, node_count, leaf_count, leaf_index_total) = (u(0), u(4), u(8), u(12));
        let bbox = ([f(16), f(20), f(24)], [f(28), f(32), f(36)]);

        let posi = coll.child(b"POSI").ok_or_else(bad)?.payload;
        if posi.len() / 12 != vertex_count as usize {
            return Err(UcfbError::Malformed(format!(
                "coll {name}: POSI holds {} vertices, INFO says {vertex_count}",
                posi.len() / 12
            )));
        }
        let vertices: Vec<[f32; 3]> = posi
            .as_chunks::<12>()
            .0
            .iter()
            .map(|c| {
                let g = |o: usize| le::f32_at(c, o);
                [g(0), g(4), g(8)]
            })
            .collect();

        let tree = coll.child(b"TREE").ok_or_else(bad)?;
        let mut polygons = Vec::new();
        let mut nodes = 0u32;
        let mut index_total = 0u32;
        for child in tree.children() {
            let child = child?;
            match &child.tag {
                b"NODE" => nodes += 1,
                b"LEAF" => {
                    let p = child.payload;
                    if p.is_empty() {
                        return Err(UcfbError::Truncated(child.offset));
                    }
                    let n = p[0] as usize;
                    if p.len() != 7 + 2 * n {
                        return Err(UcfbError::Malformed(format!(
                            "coll {name}: LEAF at 0x{:X} is {} B, expected {}",
                            child.offset,
                            p.len(),
                            7 + 2 * n
                        )));
                    }
                    let mut poly = Vec::with_capacity(n);
                    for k in 0..n {
                        let at = 7 + 2 * k;
                        let idx = u16::from_le_bytes([p[at], p[at + 1]]);
                        if u32::from(idx) >= vertex_count {
                            return Err(UcfbError::Malformed(format!(
                                "coll {name}: LEAF index {idx} >= vertexCount {vertex_count}"
                            )));
                        }
                        poly.push(idx);
                    }
                    index_total += n as u32;
                    polygons.push(poly);
                }
                _ => {}
            }
        }
        if nodes != node_count {
            return Err(UcfbError::Malformed(format!(
                "coll {name}: TREE holds {nodes} NODEs, INFO says {node_count}"
            )));
        }
        if polygons.len() as u32 != leaf_count {
            return Err(UcfbError::Malformed(format!(
                "coll {name}: TREE holds {} LEAFs, INFO says {leaf_count}",
                polygons.len()
            )));
        }
        if index_total != leaf_index_total {
            return Err(UcfbError::Malformed(format!(
                "coll {name}: leaf indices total {index_total}, INFO says {leaf_index_total}"
            )));
        }

        Ok(CollisionMesh {
            name,
            node,
            bbox,
            vertices,
            polygons,
        })
    }

    /// Fan-triangulate every polygon: (idx[0], idx[i], idx[i+1]) for
    /// i in 1..n-1, polygon order then fan order — except a group of leaves
    /// that is a SIGNED fan (a face with a hole or a notch, card STARC:
    /// [`crate::coll_fan`]), which gives the region it sums to in its first
    /// leaf's place.
    pub fn triangles(&self) -> Vec<[u32; 3]> {
        let repairs = crate::coll_fan::repairs(&self.vertices, &self.polygons);
        let mut out = Vec::new();
        for (leaf, poly) in self.polygons.iter().enumerate() {
            if let Some((group, tris)) = repairs.iter().find(|(g, _)| g.contains(&leaf)) {
                if group[0] == leaf {
                    out.extend_from_slice(tris);
                }
                continue;
            }
            if poly.len() < 3 {
                continue;
            }
            for i in 1..poly.len() - 1 {
                out.push([
                    u32::from(poly[0]),
                    u32::from(poly[i]),
                    u32::from(poly[i + 1]),
                ]);
            }
        }
        out
    }

    /// The leaves [`Self::triangles`] does not fan: each signed-fan group
    /// (ascending leaf indices) and how many triangles replace it.
    pub fn repaired(&self) -> Vec<(Vec<usize>, usize)> {
        crate::coll_fan::repairs(&self.vertices, &self.polygons)
            .into_iter()
            .map(|(g, t)| (g, t.len()))
            .collect()
    }
}

/// `prim DATA` kind. Anything but 0/2/4 is [`PrimKind::Unknown`] and carries no
/// dimensions — the shipped discs also author kind 3 (42 primitives, all-zero
/// dims, e.g. BF1 BES1 `kam_prop_antiaircraft p_collision21`) and kind 1 (once).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimKind {
    Sphere {
        radius: f32,
    },
    /// `height` is the FULL height; the cylinder is centred on the XFRM
    /// translation with its axis along the primitive's local Y.
    Cylinder {
        radius: f32,
        height: f32,
    },
    Box {
        half: [f32; 3],
    },
    Unknown(u32),
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub name: String,
    /// The `PRNT` node name in the source msh.
    pub parent: String,
    /// 3×3 rotation, row-major as stored.
    pub rotation: [[f32; 3]; 3],
    pub translation: [f32; 3],
    pub kind: PrimKind,
    /// BF2's per-primitive layer mask, as the ENGINE sees it (0 = none read =
    /// all five subsets). Bits `s = 0x01, v = 0x02, b = 0x04, t = 0x08,
    /// o = 0x10` — soldier / vehicle / building / terrain / ordnance
    /// (analysis/re-collision-layers.md §1, §2; measured over ~6,000 BF2
    /// prims against the PC exporter's `p_-so-…` letters with zero
    /// contradictions). BF1 never writes `MASK` and this is always 0 there.
    ///
    /// **The engine's carry-over quirk, replicated:** BF2's loader
    /// (`FUN_001606e8`) initialises the byte once per `prim` CHUNK, sets it
    /// only on a `MASK` child and never clears it after a `DATA`, so a
    /// primitive with no `MASK` that follows one with a `MASK` inherits that
    /// byte (209 such prims on the discs). Use [`Primitive::mask_own`] to
    /// tell an authored mask from an inherited one.
    pub mask: u8,
    /// True iff THIS primitive carried its own `MASK` chunk. Purely
    /// informational (a census can separate authored from inherited masks);
    /// the engine has no such distinction.
    pub mask_own: bool,
}

#[derive(Debug, Clone)]
pub struct Primitives {
    /// The model/geometry name from `INFO`.
    pub model: String,
    pub prims: Vec<Primitive>,
}

impl Primitives {
    pub fn parse(prim: &Chunk<'_>) -> Result<Primitives, UcfbError> {
        let bad = || UcfbError::Truncated(prim.offset);
        let info = prim.child(b"INFO").ok_or_else(bad)?.payload;
        let nul = info.iter().position(|&b| b == 0).ok_or_else(bad)?;
        let model = std::str::from_utf8(&info[..nul])
            .map_err(|_| UcfbError::Malformed(format!("prim 0x{:X}: INFO name", prim.offset)))?
            .to_owned();
        if info.len() < nul + 5 {
            return Err(bad());
        }
        let count = u32::from_le_bytes(info[nul + 1..nul + 5].try_into().unwrap());

        // NAME opens a primitive; MASK/PRNT/XFRM/DATA fill it and DATA closes
        // it. `mask_temp` is the engine's running byte: set only by `MASK`,
        // never cleared between primitives (see `Primitive::mask`).
        let mut prims: Vec<Primitive> = Vec::new();
        let mut pending: Option<Primitive> = None;
        let mut mask_temp: u8 = 0;
        let mut mask_own = false;
        for child in prim.children() {
            let child = child?;
            match &child.tag {
                b"NAME" => {
                    let name = child.as_str().ok_or_else(bad)?.to_owned();
                    mask_own = false;
                    pending = Some(Primitive {
                        name,
                        parent: String::new(),
                        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                        translation: [0.0; 3],
                        kind: PrimKind::Unknown(u32::MAX),
                        mask: 0,
                        mask_own: false,
                    });
                }
                // BF2 only, one byte, between NAME and PRNT.
                b"MASK" => {
                    if child.payload.is_empty() {
                        return Err(UcfbError::Truncated(child.offset));
                    }
                    mask_temp = child.payload[0];
                    mask_own = true;
                }
                b"PRNT" => {
                    if let Some(p) = pending.as_mut() {
                        p.parent = child.as_str().unwrap_or_default().to_owned();
                    }
                }
                b"XFRM" => {
                    if child.payload.len() < 48 {
                        return Err(UcfbError::Truncated(child.offset));
                    }
                    let g = |o: usize| le::f32_at(child.payload, o);
                    if let Some(p) = pending.as_mut() {
                        p.rotation = [
                            [g(0), g(4), g(8)],
                            [g(12), g(16), g(20)],
                            [g(24), g(28), g(32)],
                        ];
                        p.translation = [g(36), g(40), g(44)];
                    }
                }
                b"DATA" => {
                    if child.payload.len() < 16 {
                        return Err(UcfbError::Truncated(child.offset));
                    }
                    let g = |o: usize| le::f32_at(child.payload, o);
                    let kind = u32::from_le_bytes(child.payload[0..4].try_into().unwrap());
                    let (a, b, c) = (g(4), g(8), g(12));
                    if let Some(mut p) = pending.take() {
                        // The engine applies the running byte at DATA, whether
                        // or not THIS primitive authored one.
                        p.mask = mask_temp;
                        p.mask_own = mask_own;
                        p.kind = match kind {
                            0 => PrimKind::Sphere { radius: a },
                            2 => PrimKind::Cylinder {
                                radius: a,
                                height: b,
                            },
                            4 => PrimKind::Box { half: [a, b, c] },
                            other => PrimKind::Unknown(other),
                        };
                        prims.push(p);
                    }
                }
                _ => {}
            }
        }
        if prims.len() as u32 != count {
            return Err(UcfbError::Malformed(format!(
                "prim {model}: {} primitives, INFO says {count}",
                prims.len()
            )));
        }
        Ok(Primitives { model, prims })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_chunk;

    fn coll_chunk(
        name: &str,
        node: &str,
        counts: (u32, u32, u32, u32),
        verts: &[[f32; 3]],
        leaves: &[Vec<u16>],
        nodes: usize,
    ) -> Vec<u8> {
        let mut info = Vec::new();
        for v in [counts.0, counts.1, counts.2, counts.3] {
            info.extend(v.to_le_bytes());
        }
        for v in [-1.0f32, -2.0, -3.0, 4.0, 5.0, 6.0] {
            info.extend(v.to_le_bytes());
        }
        let mut posi = Vec::new();
        for v in verts {
            for c in v {
                posi.extend(f32::to_le_bytes(*c));
            }
        }
        let mut tree = Vec::new();
        for _ in 0..nodes {
            tree.extend(build_chunk(b"NODE", &[0u8; 6]));
        }
        for leaf in leaves {
            let mut p = vec![leaf.len() as u8];
            p.extend([0xffu8, 0x99, 0xff, 0x1d, 0x00, 0x00]);
            for i in leaf {
                p.extend(i.to_le_bytes());
            }
            tree.extend(build_chunk(b"LEAF", &p));
        }
        let mut body = Vec::new();
        body.extend(build_chunk(b"NAME", format!("{name}\0").as_bytes()));
        body.extend(build_chunk(b"NODE", format!("{node}\0").as_bytes()));
        body.extend(build_chunk(b"INFO", &info));
        body.extend(build_chunk(b"POSI", &posi));
        body.extend(build_chunk(b"TREE", &tree));
        build_chunk(b"coll", &body)
    }

    fn first(buf: &[u8]) -> Chunk<'_> {
        crate::chunks(buf).next().unwrap().unwrap()
    }

    #[test]
    fn synthetic_coll_fans_a_triangle_and_a_quad() {
        let verts = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [2.0, 0.0, 0.0],
        ];
        let leaves = vec![vec![0u16, 1, 2], vec![0u16, 1, 4, 3]];
        let buf = coll_chunk("geo", "geo_export", (5, 3, 2, 7), &verts, &leaves, 3);
        let m = CollisionMesh::parse(&first(&buf)).unwrap();
        assert_eq!(m.name, "geo");
        assert_eq!(m.node, "geo_export");
        assert_eq!(m.bbox, ([-1.0, -2.0, -3.0], [4.0, 5.0, 6.0]));
        assert_eq!(m.vertices.len(), 5);
        assert_eq!(m.polygons, leaves);
        // Fan order: the triangle, then the quad's two fan triangles.
        assert_eq!(m.triangles(), vec![[0, 1, 2], [0, 1, 4], [0, 4, 3]]);
    }

    #[test]
    fn truncated_leaf_is_an_error_not_a_panic() {
        // A LEAF claiming 4 indices but carrying 3.
        let verts = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]];
        let mut info = Vec::new();
        for v in [3u32, 0, 1, 4] {
            info.extend(v.to_le_bytes());
        }
        info.extend([0u8; 24]);
        let mut posi = Vec::new();
        for v in &verts {
            for c in v {
                posi.extend(f32::to_le_bytes(*c));
            }
        }
        let mut leaf = vec![4u8];
        leaf.extend([0u8; 6]);
        for i in [0u16, 1, 2] {
            leaf.extend(i.to_le_bytes());
        }
        let mut body = Vec::new();
        body.extend(build_chunk(b"NAME", b"geo\0"));
        body.extend(build_chunk(b"NODE", b"n\0"));
        body.extend(build_chunk(b"INFO", &info));
        body.extend(build_chunk(b"POSI", &posi));
        body.extend(build_chunk(b"TREE", &build_chunk(b"LEAF", &leaf)));
        let buf = build_chunk(b"coll", &body);
        assert!(matches!(
            CollisionMesh::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    #[test]
    fn out_of_range_leaf_index_is_an_error() {
        let verts = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]];
        let buf = coll_chunk("geo", "n", (3, 0, 1, 3), &verts, &[vec![0u16, 1, 9]], 0);
        assert!(matches!(
            CollisionMesh::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    #[test]
    fn info_count_mismatch_is_an_error() {
        let verts = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]];
        // INFO says 4 vertices, POSI holds 3.
        let buf = coll_chunk("geo", "n", (4, 0, 1, 3), &verts, &[vec![0u16, 1, 2]], 0);
        assert!(matches!(
            CollisionMesh::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    fn prim_chunk(model: &str, entries: &[(&str, &str, u32, [f32; 3])]) -> Vec<u8> {
        let mut info = Vec::new();
        info.extend(model.as_bytes());
        info.push(0);
        info.extend((entries.len() as u32).to_le_bytes());
        let mut body = build_chunk(b"INFO", &info);
        for (name, parent, kind, dims) in entries {
            body.extend(build_chunk(b"NAME", format!("{name}\0").as_bytes()));
            body.extend(build_chunk(b"PRNT", format!("{parent}\0").as_bytes()));
            let mut x = Vec::new();
            for v in [
                1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.5, 2.5, 3.5,
            ] {
                x.extend(v.to_le_bytes());
            }
            body.extend(build_chunk(b"XFRM", &x));
            let mut d = kind.to_le_bytes().to_vec();
            for v in dims {
                d.extend(v.to_le_bytes());
            }
            body.extend(build_chunk(b"DATA", &d));
        }
        build_chunk(b"prim", &body)
    }

    #[test]
    fn synthetic_prim_reads_every_kind() {
        let buf = prim_chunk(
            "geo",
            &[
                ("p_sphere", "root", 0, [2.0, 0.0, 0.0]),
                ("p_collision", "root", 2, [1.5, 8.0, 0.0]),
                ("p_cube", "root", 4, [1.0, 2.0, 3.0]),
                ("p_collision21", "root", 3, [0.0, 0.0, 0.0]),
            ],
        );
        let p = Primitives::parse(&first(&buf)).unwrap();
        assert_eq!(p.model, "geo");
        assert_eq!(p.prims.len(), 4);
        assert_eq!(p.prims[0].kind, PrimKind::Sphere { radius: 2.0 });
        assert_eq!(
            p.prims[1].kind,
            PrimKind::Cylinder {
                radius: 1.5,
                height: 8.0
            }
        );
        assert_eq!(
            p.prims[2].kind,
            PrimKind::Box {
                half: [1.0, 2.0, 3.0]
            }
        );
        assert_eq!(p.prims[3].kind, PrimKind::Unknown(3));
        assert_eq!(p.prims[0].parent, "root");
        assert_eq!(p.prims[0].translation, [1.5, 2.5, 3.5]);
        assert_eq!(p.prims[0].rotation[1], [0.0, 1.0, 0.0]);
    }

    /// `prim` chunk with an optional `MASK` per entry, in the disc's order
    /// (NAME, MASK, PRNT, XFRM, DATA).
    fn prim_chunk_masked(model: &str, entries: &[(&str, Option<u8>)]) -> Vec<u8> {
        let mut info = Vec::new();
        info.extend(model.as_bytes());
        info.push(0);
        info.extend((entries.len() as u32).to_le_bytes());
        let mut body = build_chunk(b"INFO", &info);
        for (name, mask) in entries {
            body.extend(build_chunk(b"NAME", format!("{name}\0").as_bytes()));
            if let Some(m) = mask {
                body.extend(build_chunk(b"MASK", &[*m]));
            }
            body.extend(build_chunk(b"PRNT", b"root\0"));
            let mut x = Vec::new();
            for v in [
                1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
            ] {
                x.extend(v.to_le_bytes());
            }
            body.extend(build_chunk(b"XFRM", &x));
            let mut d = 4u32.to_le_bytes().to_vec();
            for v in [1.0f32, 1.0, 1.0] {
                d.extend(v.to_le_bytes());
            }
            body.extend(build_chunk(b"DATA", &d));
        }
        build_chunk(b"prim", &body)
    }

    /// The BF2 loader's stale-MASK quirk (analysis/re-collision-layers.md §1):
    /// the byte is set only by `MASK` and never cleared, so an unmasked prim
    /// after a masked one inherits it. 209 prims on the discs depend on this.
    #[test]
    fn mask_carries_over_to_a_primitive_that_has_none() {
        let buf = prim_chunk_masked(
            "geo",
            &[
                ("p_-so-a", Some(0x11)),
                ("p_cube", None),
                ("p_-v-b", Some(0x02)),
                ("p_cube1", None),
            ],
        );
        let p = Primitives::parse(&first(&buf)).unwrap();
        let got: Vec<(&str, u8, bool)> = p
            .prims
            .iter()
            .map(|x| (x.name.as_str(), x.mask, x.mask_own))
            .collect();
        assert_eq!(
            got,
            vec![
                ("p_-so-a", 0x11, true),
                ("p_cube", 0x11, false),
                ("p_-v-b", 0x02, true),
                ("p_cube1", 0x02, false),
            ]
        );
    }

    /// A chunk with no `MASK` anywhere — every BF1 chunk, and 9 of 21 BF2 map
    /// LVLs — reads back mask 0 (= all five subsets).
    #[test]
    fn a_chunk_without_masks_reads_zero() {
        let buf = prim_chunk_masked("geo", &[("p_cube", None), ("p_cube1", None)]);
        let p = Primitives::parse(&first(&buf)).unwrap();
        assert!(p.prims.iter().all(|x| x.mask == 0 && !x.mask_own));
    }

    #[test]
    fn prim_count_mismatch_is_an_error() {
        let mut buf = prim_chunk("geo", &[("p_cube", "root", 4, [1.0, 1.0, 1.0])]);
        // Rewrite INFO's count to 2 without adding a second primitive.
        let at = buf
            .windows(4)
            .position(|w| w == b"INFO")
            .expect("INFO present")
            + 8
            + "geo\0".len();
        buf[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(matches!(
            Primitives::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    /// Card STARC, data-gated: the signed fans on the discs' baked maps.
    /// BF2 POL1's radar-room wall keeps its doorway (five leaves -> the wall
    /// less the hole, 8 triangles). Left as fans: BF2 MUS1's
    /// `mus1_bldg_world_1` ten-sided ground leaf (not flat: its apex is 1.3 m
    /// over the rest, and its outline crosses itself — not understood), and
    /// BF1 TAT1's concave leaf, whose fan is its own region.
    #[test]
    fn the_discs_signed_fans_are_repaired() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../extracted/");
        let repaired = |lvl: &str, name: &str| -> Option<Vec<(Vec<usize>, usize)>> {
            let buf = std::fs::read(format!("{root}{lvl}")).ok()?;
            let inner = crate::open(&buf).unwrap();
            let mut found = None;
            for c in crate::chunks(inner).flatten() {
                if &c.tag == b"coll" {
                    let m = CollisionMesh::parse(&c).unwrap();
                    if m.name == name {
                        // The fan count less the leaves' own, plus the repair's.
                        let fans: usize = m.polygons.iter().map(|p| p.len() - 2).sum();
                        let r = m.repaired();
                        let gone: usize = r
                            .iter()
                            .flat_map(|(g, _)| g.iter().map(|&l| m.polygons[l].len() - 2))
                            .sum();
                        let new: usize = r.iter().map(|(_, n)| n).sum();
                        assert_eq!(m.triangles().len(), fans - gone + new);
                        found = Some(r);
                    }
                }
            }
            found
        };
        let Some(pol) = repaired(
            "bf2/lvl/DATA/_LVL_PS2/POL/POL1.LVL",
            "pol1_bldg_radar_main01",
        ) else {
            eprintln!("SKIP: extracted BF2 POL1 not present");
            return;
        };
        assert_eq!(pol, vec![(vec![46, 49, 50, 51, 52], 8)]);
        let mus = repaired("bf2/lvl/DATA/_LVL_PS2/MUS/MUS1.LVL", "mus1_bldg_world_1").unwrap();
        assert!(mus.is_empty(), "{mus:?}");
        let tat = repaired(
            "bf1/lvl/DATA/_LVL_PS2/TAT/TAT1.LVL",
            "tat1_bldg_rock_rubble_pillar3",
        )
        .unwrap();
        assert!(tat.is_empty(), "{tat:?}");
    }

    /// Pinned vectors measured on the raw disc (RE_LOG 2026-09-01
    /// coll/prim/CUTR measurement, Fable session 7). Data-gated.
    #[test]
    fn kam1_pinned_coll_and_prim_vectors() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/KAM/KAM1.LVL"
        );
        if !std::path::Path::new(path).exists() {
            eprintln!("SKIP: extracted BF1 KAM1 not present");
            return;
        }
        let buf = std::fs::read(path).unwrap();
        let inner = crate::open(&buf).unwrap();
        let mut bunker = None;
        let mut pillar2 = None;
        let mut pillar = None;
        for c in crate::chunks(inner).flatten() {
            // `chunks` counts from the ucfb payload; the pinned offsets are
            // FILE offsets, i.e. 8 bytes further on.
            let file_offset = c.offset + 8;
            match &c.tag {
                b"coll" if file_offset == 0x1BA228 => {
                    bunker = Some(CollisionMesh::parse(&c).unwrap());
                }
                b"prim" if file_offset == 0xD7F18 => {
                    pillar2 = Some(Primitives::parse(&c).unwrap());
                }
                b"prim" if file_offset == 0xD9EF4 => {
                    pillar = Some(Primitives::parse(&c).unwrap());
                }
                _ => {}
            }
        }
        let m = bunker.expect("coll at 0x1BA228");
        assert_eq!(m.name, "kam_bldg_bunker1");
        assert_eq!(m.node, "bunker_export");
        assert_eq!(m.vertices.len(), 223);
        assert_eq!(m.polygons.len(), 217);
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4);
        assert!(
            near(m.bbox.0, [-9.366828, -2.9212205, -53.44321]),
            "{:?}",
            m.bbox.0
        );
        assert!(
            near(m.bbox.1, [9.365_232, 17.206034, -19.511934]),
            "{:?}",
            m.bbox.1
        );
        assert!(
            near(m.vertices[0], [1.8961419, 0.88118327, -21.585752]),
            "{:?}",
            m.vertices[0]
        );
        assert!(
            near(m.vertices[222], [4.874606, 7.1051407, -40.51262]),
            "{:?}",
            m.vertices[222]
        );
        assert_eq!(m.polygons[0], vec![29, 40, 25]);
        // leafIndexTotal 820 over 217 leaves: 820 − 2·217 = 386 fan triangles.
        assert_eq!(m.triangles().len(), 386);

        let p2 = pillar2.expect("prim at 0xD7F18");
        assert_eq!(p2.model, "kam_prop_pillar2");
        assert_eq!(p2.prims.len(), 1);
        assert_eq!(p2.prims[0].name, "p_collision1");
        assert_eq!(p2.prims[0].parent, "pillar2");
        assert_eq!(
            p2.prims[0].rotation,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert!(
            near(p2.prims[0].translation, [15.647264, 2.1523004, -10.661763]),
            "{:?}",
            p2.prims[0].translation
        );
        match p2.prims[0].kind {
            PrimKind::Box { half } => {
                assert!(near(half, [0.38737866, 2.3135357, 0.37333333]), "{half:?}")
            }
            k => panic!("{k:?}"),
        }

        let p1 = pillar.expect("prim at 0xD9EF4");
        assert_eq!(p1.model, "kam_prop_pillar");
        assert_eq!(p1.prims[0].name, "p_antena");
        assert_eq!(
            p1.prims[0].kind,
            PrimKind::Cylinder {
                radius: 1.1,
                height: 20.0
            }
        );
        assert!(
            near(p1.prims[0].translation, [14.143256, 9.689061, -13.764727]),
            "{:?}",
            p1.prims[0].translation
        );
    }

    /// Pinned BF2 `MASK` vectors, measured on the raw disc with
    /// `scripts/ucfb_tree.py --tag prim --peek 16` (offsets are FILE offsets
    /// of the chunk header): MYG1.LVL `myg1_bldg_large_platform` @0xD6618 —
    /// `p_-so-rob` MASK 0x11 @0xD665C, `p_-svo-rob8` MASK 0x13 @0xD67F4, the
    /// `p_-v-rob*` boxes 0x02 — and `myg1_bldg_small_spiral` @0x1065AC, whose
    /// `p_-v-rob` MASK 0x02 @0x10667C is the cylinder that held MYG1 unit 6.
    /// The letter-to-bit mapping is analysis/re-collision-layers.md §2.
    /// Data-gated, like the KAM1 vectors above.
    #[test]
    fn myg1_pinned_prim_masks() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/MYG/MYG1.LVL"
        );
        if !std::path::Path::new(path).exists() {
            eprintln!("SKIP: extracted BF2 MYG1 not present");
            return;
        }
        let buf = std::fs::read(path).unwrap();
        let inner = crate::open(&buf).unwrap();
        let mut platform = None;
        let mut spiral = None;
        for c in crate::chunks(inner).flatten() {
            let file_offset = c.offset + 8;
            match &c.tag {
                b"prim" if file_offset == 0xD6618 => {
                    platform = Some(Primitives::parse(&c).unwrap());
                }
                b"prim" if file_offset == 0x1065AC => {
                    spiral = Some(Primitives::parse(&c).unwrap());
                }
                _ => {}
            }
        }
        let p = platform.expect("prim at 0xD6618");
        assert_eq!(p.model, "myg1_bldg_large_platform");
        assert_eq!(p.prims.len(), 16);
        // Every primitive of this chunk authors its own MASK.
        assert!(p.prims.iter().all(|x| x.mask_own));
        assert_eq!(
            (p.prims[0].name.as_str(), p.prims[0].mask),
            ("p_-so-rob", 0x11)
        );
        assert_eq!(
            (p.prims[1].name.as_str(), p.prims[1].mask),
            ("p_-v-rob18", 0x02)
        );
        assert_eq!(
            (p.prims[3].name.as_str(), p.prims[3].mask),
            ("p_-svo-rob8", 0x13)
        );
        // The letters and the byte agree on all 16, which is what §2 measured
        // (s = 0x01, v = 0x02, o = 0x10).
        for prim in &p.prims {
            let n = prim.name.as_str();
            let letters = n
                .strip_prefix("p_-")
                .and_then(|r| r.split_once('-'))
                .expect(n)
                .0;
            let expect: u8 = letters
                .bytes()
                .map(|c| match c {
                    b's' => 0x01,
                    b'v' => 0x02,
                    b'b' => 0x04,
                    b't' => 0x08,
                    b'o' => 0x10,
                    other => panic!("unexpected letter {}", other as char),
                })
                .sum();
            assert_eq!(prim.mask, expect, "{n}");
        }

        let s = spiral.expect("prim at 0x1065AC");
        assert_eq!(s.model, "myg1_bldg_small_spiral");
        assert_eq!(s.prims.len(), 2);
        assert_eq!(
            (s.prims[0].name.as_str(), s.prims[0].mask),
            ("p_-so-rob1", 0x11)
        );
        assert_eq!(
            (s.prims[1].name.as_str(), s.prims[1].mask),
            ("p_-v-rob", 0x02)
        );
        assert!(matches!(s.prims[1].kind, PrimKind::Cylinder { .. }));
    }

    /// BF1 ships no `MASK` chunk at all (§1: zero hits for 0x4B53414D in the
    /// BF1 decomp), so every BF1 primitive reads back mask 0 = all five
    /// subsets. Data-gated on KAM1.
    #[test]
    fn bf1_prims_carry_no_mask() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/KAM/KAM1.LVL"
        );
        if !std::path::Path::new(path).exists() {
            eprintln!("SKIP: extracted BF1 KAM1 not present");
            return;
        }
        let buf = std::fs::read(path).unwrap();
        let inner = crate::open(&buf).unwrap();
        let mut chunks = 0usize;
        let mut prims = 0usize;
        for c in crate::chunks(inner).flatten() {
            if &c.tag != b"prim" {
                continue;
            }
            let p = Primitives::parse(&c).unwrap();
            chunks += 1;
            prims += p.prims.len();
            assert!(
                p.prims.iter().all(|x| x.mask == 0 && !x.mask_own),
                "{} carries a MASK",
                p.model
            );
        }
        assert!(chunks > 0 && prims > 0);
    }
}
