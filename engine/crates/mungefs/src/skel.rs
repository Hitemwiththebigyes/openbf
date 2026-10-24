//! Model skeletons (`skel`): the bind pose the soldier meshes are stored in.
//!
//! Layout (analysis/re-softskin.md §2, lead-verified on IMP.LVL raw bytes):
//! `INFO` = cstring name + u32 boneCount (variable size); `NAME` = N packed
//! cstrings in bone order; `PRNT` = N packed cstrings of PARENT NAMES ("" =
//! root); `XFRM` = N × 12 f32 = rows r0, r1, r2 of a 3×3 rotation + t,
//! local-to-parent, ROW-VECTOR convention (`v_parent = v_local · R + t`).
//! Bone identity everywhere is the engine CRC-32 of the lowercased name
//! (`anim::crc32_lower`). The bind pose is a T-pose, Y up, +X = the
//! character's left, +Z forward (inferred from toe vs foot).

use crate::{le, Chunk, UcfbError};

#[derive(Debug, Clone, PartialEq)]
pub struct SkelBone {
    pub name: String,
    pub crc: u32,
    pub parent: Option<usize>,
    /// Rotation rows r0, r1, r2 (row-vector convention).
    pub rows: [[f32; 3]; 3],
    /// Translation in the parent's space.
    pub t: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    pub name: String,
    pub bones: Vec<SkelBone>,
}

/// Row-vector 4×4 (rows; `v' = v · M`).
pub type RowMat4 = [[f32; 4]; 4];

pub fn mul_row(a: &RowMat4, b: &RowMat4) -> RowMat4 {
    let mut m = [[0.0f32; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// Transform a point by a row-vector matrix.
pub fn xform_point(m: &RowMat4, p: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (j, o) in out.iter_mut().enumerate() {
        *o = p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j];
    }
    out
}

/// Rotate a direction by the 3×3 part of a row-vector matrix.
pub fn xform_dir(m: &RowMat4, d: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (j, o) in out.iter_mut().enumerate() {
        *o = d[0] * m[0][j] + d[1] * m[1][j] + d[2] * m[2][j];
    }
    out
}

impl SkelBone {
    /// Local bind transform as a row-vector 4×4.
    pub fn local(&self) -> RowMat4 {
        let r = self.rows;
        [
            [r[0][0], r[0][1], r[0][2], 0.0],
            [r[1][0], r[1][1], r[1][2], 0.0],
            [r[2][0], r[2][1], r[2][2], 0.0],
            [self.t[0], self.t[1], self.t[2], 1.0],
        ]
    }
}

impl Skeleton {
    pub fn index_of(&self, name: &str) -> Option<usize> {
        let crc = crate::anim::crc32_lower(name);
        self.bones.iter().position(|b| b.crc == crc)
    }

    /// Bind bone→model matrices `M0[i] = L[i] × M0[parent(i)]` (root parent =
    /// identity), row-vector convention — what the VU receives at rest.
    ///
    /// Errors on a PRNT that does not form a tree. Parents resolve by name
    /// CRC, so a bone naming itself (or two bones sharing a name) makes a
    /// cycle, which has no bind pose. Walking the chain iteratively also keeps
    /// a long (valid) parent chain off the call stack — a crafted `skel` can
    /// declare tens of thousands of bones, which recursion would overflow on.
    pub fn bind_matrices(&self) -> Result<Vec<RowMat4>, UcfbError> {
        let n = self.bones.len();
        let mut out: Vec<Option<RowMat4>> = vec![None; n];
        for i in 0..n {
            // Collect the unresolved run from `i` up towards its root, then
            // fill it in root-first. More than `n` links means the chain
            // re-entered a bone it had already queued, i.e. a cycle.
            let mut chain: Vec<usize> = Vec::new();
            let mut cur = Some(i);
            while let Some(c) = cur {
                if out[c].is_some() {
                    break;
                }
                if chain.len() > n {
                    return Err(UcfbError::Malformed(format!(
                        "skel {}: parent cycle at bone {c}",
                        self.name
                    )));
                }
                chain.push(c);
                cur = self.bones[c].parent;
                if let Some(p) = cur {
                    if p >= n {
                        return Err(UcfbError::Malformed(format!(
                            "skel {}: bone {c} parent {p} out of range",
                            self.name
                        )));
                    }
                }
            }
            for &c in chain.iter().rev() {
                let local = self.bones[c].local();
                out[c] = Some(match self.bones[c].parent {
                    Some(p) => match out.get(p).copied().flatten() {
                        Some(pm) => mul_row(&local, &pm),
                        // Unreachable for a tree (the parent was filled in
                        // just above), but an out-of-range PRNT index must
                        // not index-panic.
                        None => {
                            return Err(UcfbError::Malformed(format!(
                                "skel {}: bone {c} parent {p} out of range",
                                self.name
                            )))
                        }
                    },
                    None => local,
                });
            }
        }
        out.into_iter()
            .enumerate()
            .map(|(i, m)| {
                m.ok_or_else(|| {
                    UcfbError::Malformed(format!("skel {}: bone {i} unresolved", self.name))
                })
            })
            .collect()
    }
}

fn cstrings(payload: &[u8], n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    let mut at = 0;
    for _ in 0..n {
        let end = payload[at..]
            .iter()
            .position(|&b| b == 0)
            .map_or(payload.len(), |e| at + e);
        out.push(String::from_utf8_lossy(&payload[at..end]).into_owned());
        at = (end + 1).min(payload.len());
    }
    out
}

pub fn parse_skel(skel: &Chunk<'_>) -> Result<Skeleton, UcfbError> {
    let bad = || UcfbError::Truncated(skel.offset);
    let info = skel.child(b"INFO").ok_or_else(bad)?.payload;
    let name_end = info.iter().position(|&b| b == 0).ok_or_else(bad)?;
    let name = String::from_utf8_lossy(&info[..name_end]).into_owned();
    let count = info
        .get(name_end + 1..name_end + 5)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .ok_or_else(bad)?;
    // XFRM bounds `count` before anything is sized by it.
    let xfrm = skel.child(b"XFRM").ok_or_else(bad)?.payload;
    if xfrm.len() / 48 < count {
        return Err(bad());
    }
    let names = cstrings(skel.child(b"NAME").ok_or_else(bad)?.payload, count);
    let parents = cstrings(skel.child(b"PRNT").ok_or_else(bad)?.payload, count);
    let f = |at: usize| le::f32_at(xfrm, at);
    let crcs: Vec<u32> = names.iter().map(|n| crate::anim::crc32_lower(n)).collect();
    let bones = (0..count)
        .map(|i| {
            let b = i * 48;
            let row = |r: usize| [f(b + r * 12), f(b + r * 12 + 4), f(b + r * 12 + 8)];
            let parent = if parents[i].is_empty() {
                None
            } else {
                let pc = crate::anim::crc32_lower(&parents[i]);
                crcs.iter().position(|&c| c == pc)
            };
            SkelBone {
                name: names[i].clone(),
                crc: crcs[i],
                parent,
                rows: [row(0), row(1), row(2)],
                t: row(3),
            }
        })
        .collect();
    Ok(Skeleton { name, bones })
}

/// Every `skel` in an LVL (via `content_chunks`).
pub fn find_skeletons(inner: &[u8]) -> Result<Vec<Skeleton>, UcfbError> {
    crate::content_chunks(inner)?
        .iter()
        .filter(|c| &c.tag == b"skel")
        .map(parse_skel)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close3(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < tol)
    }

    /// re-softskin.md §10 vectors 1–5 (BF1 IMP.LVL stormtrooper skeleton).
    #[test]
    fn imp_lvl_stormtrooper_skeleton() {
        let p = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/SIDE/IMP.LVL"
        );
        let Ok(buf) = std::fs::read(p) else {
            eprintln!("SKIP: IMP.LVL not present");
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let skels = find_skeletons(inner).unwrap();
        let s = skels
            .iter()
            .find(|s| s.name == "imp_inf_stormtrooper")
            .unwrap();
        assert_eq!(s.bones.len(), 25);
        assert_eq!(s.bones[0].name, "DummyRoot");
        assert_eq!(s.bones[1].name, "bone_ribcage");
        assert_eq!(s.bones[20].name, "bone_root");
        assert_eq!(s.bones[23].name, "hp_weapons");
        assert_eq!(s.bones[24].name, "bone_neck");
        assert_eq!(s.bones[0].parent, None);
        assert_eq!(s.bones[1].parent, Some(5));
        assert_eq!(s.bones[20].parent, Some(0));
        assert_eq!(s.bones[12].parent, Some(11));
        assert_eq!(s.bones[1].crc, 0xE5B5_1072);
        assert_eq!(s.index_of("BONE_ROOT"), Some(20));
        assert!(close3(s.bones[20].t, [0.0, 1.1593735, 0.0], 1e-6));
        assert!(close3(s.bones[1].t, [0.1592044, -2.68e-5, 0.0], 1e-5));
        assert!(close3(
            s.bones[1].rows[0],
            [0.9970601, 0.0766236, 0.0],
            1e-6
        ));
        assert!(close3(
            s.bones[12].t,
            [-0.0942804, -0.1049374, 0.0091177],
            1e-6
        ));
        let m0 = s.bind_matrices().unwrap();
        let model_t = |i: usize| [m0[i][3][0], m0[i][3][1], m0[i][3][2]];
        assert!(
            close3(model_t(1), [0.0, 1.4811, -0.0228], 1e-3),
            "{:?}",
            model_t(1)
        );
        assert!(close3(model_t(13), [-0.1694, 0.5806, 0.0238], 1e-3));
        assert!(close3(model_t(21), [0.7541, 1.6240, -0.0164], 1e-3));
        assert!(close3(model_t(10), [0.0, 1.8437, 0.0129], 1e-3));
    }

    /// §10 vector 11 (BF2 REP.LVL ep3 trooper skeleton, different bone order).
    #[test]
    fn rep_lvl_ep3trooper_skeleton() {
        let p = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/SIDE/REP.LVL"
        );
        let Ok(buf) = std::fs::read(p) else {
            eprintln!("SKIP: REP.LVL not present");
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let skels = find_skeletons(inner).unwrap();
        let s = skels
            .iter()
            .find(|s| s.name == "rep_inf_ep3trooper")
            .unwrap();
        assert_eq!(s.bones.len(), 25);
        assert_eq!(s.bones[1].name, "bone_b_spine");
        assert_eq!(s.bones[1].parent, Some(22));
        assert_eq!(s.bones[22].name, "bone_a_spine");
        let m0 = s.bind_matrices().unwrap();
        assert!(close3(
            [m0[1][3][0], m0[1][3][1], m0[1][3][2]],
            [0.0, 1.32217, -0.01415],
            1e-4
        ));
        assert!(close3(
            [m0[3][3][0], m0[3][3][1], m0[3][3][2]],
            [0.0, 1.7162, -0.01769],
            1e-4
        ));
        assert_eq!(s.bones[3].name, "bone_l_clavicle");
    }

    #[test]
    fn row_matrix_helpers() {
        let id: RowMat4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let t: RowMat4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 2.0, 3.0, 1.0],
        ];
        assert_eq!(mul_row(&id, &t), t);
        assert_eq!(xform_point(&t, [1.0, 1.0, 1.0]), [2.0, 3.0, 4.0]);
        assert_eq!(xform_dir(&t, [1.0, 1.0, 1.0]), [1.0, 1.0, 1.0]);
    }

    /// A `skel` chunk with the given bone names and parent names, XFRM zeroed.
    fn skel_chunk(names: &[&str], parents: &[&str]) -> Vec<u8> {
        use crate::build_chunk;
        let packed = |v: &[&str]| {
            let mut o = Vec::new();
            for s in v {
                o.extend(s.as_bytes());
                o.push(0);
            }
            o
        };
        let mut info = b"sk\0".to_vec();
        info.extend((names.len() as u32).to_le_bytes());
        let mut body = build_chunk(b"INFO", &info);
        body.extend(build_chunk(b"NAME", &packed(names)));
        body.extend(build_chunk(b"PRNT", &packed(parents)));
        body.extend(build_chunk(b"XFRM", &vec![0u8; names.len() * 48]));
        build_chunk(b"skel", &body)
    }

    fn parse_bytes(buf: &[u8]) -> Skeleton {
        let c = crate::chunks(buf).next().unwrap().unwrap();
        parse_skel(&c).unwrap()
    }

    /// Parents resolve by name CRC, so a bone naming itself — or two bones
    /// naming each other — makes a cycle with no bind pose. That used to trip
    /// an `assert!` inside `bind_matrices`.
    #[test]
    fn parent_cycle_errors_instead_of_asserting() {
        let s = parse_bytes(&skel_chunk(&["a"], &["a"]));
        assert_eq!(s.bones[0].parent, Some(0), "self-parent");
        let e = s.bind_matrices().unwrap_err();
        assert!(matches!(e, UcfbError::Malformed(_)), "{e}");

        let s = parse_bytes(&skel_chunk(&["a", "b"], &["b", "a"]));
        let e = s.bind_matrices().unwrap_err();
        assert!(matches!(e, UcfbError::Malformed(_)), "{e}");
    }

    #[test]
    fn out_of_range_parent_errors_before_traversal() {
        let mut s = parse_bytes(&skel_chunk(&["a"], &[""]));
        for parent in [1, usize::MAX] {
            s.bones[0].parent = Some(parent);
            let e = s.bind_matrices().unwrap_err();
            assert!(matches!(e, UcfbError::Malformed(_)), "{e}");
            assert!(e
                .to_string()
                .contains(&format!("bone 0 parent {parent} out of range")));
        }
    }

    /// A long but valid parent chain: `bind_matrices` used to recurse once per
    /// link, so a `skel` declaring tens of thousands of bones overflowed the
    /// stack. Resolving iteratively keeps it off the call stack.
    #[test]
    fn long_parent_chain_does_not_overflow() {
        let n = 100_000usize;
        // Bone i is parented to i + 1, so resolving bone 0 walks the whole
        // chain in one go (the file dictates the order; it need not be
        // root-first).
        let bones = (0..n)
            .map(|i| SkelBone {
                name: format!("b{i}"),
                crc: i as u32,
                parent: if i + 1 == n { None } else { Some(i + 1) },
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                t: [1.0, 0.0, 0.0],
            })
            .collect();
        let s = Skeleton {
            name: "chain".into(),
            bones,
        };
        let m0 = s.bind_matrices().expect("a chain is a tree, not a cycle");
        assert_eq!(m0.len(), n);
        // Each link translates +1 in x, so bone 0 sits n units out.
        assert_eq!(m0[0][3][0], n as f32);
        assert_eq!(m0[n - 1][3][0], 1.0);
    }
}
