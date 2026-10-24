//! `plan` (the authored AI planning graph) and `plnp` (BF2's dynamic-planning
//! patches for it) — the data that IS the router on both discs. There is no
//! graph search anywhere in either ELF: the authored tables pick the next arc.
//!
//! Measured on the raw discs: RE_LOG 2026-09-01 "`plan` / `plnp` / `Hint` /
//! `BARR`" and its SPEC block. Layouts (all little-endian, offsets into the
//! chunk payload):
//!
//! ```text
//! plan
//!   INFO  u16 hubCount; u16 arcCount; u16 typeCount (= 5 on all 55 shipped
//!         chunks; BOTH loaders read it and throw it away — the 5 unit-type
//!         tables are hard-coded)
//!   NODE  hubCount records, in order. TWO layouts:
//!         BF1, fixed 40 + 20*hubCount B:
//!           +0  char name[16]  ("HubN"; both loaders DISCARD it)
//!           +16 f32 x, y, z    world frame as stored
//!           +28 f32 radius
//!           +32 u8  arcSlot[8] index into ARCS, 0xff = empty (prefix-packed)
//!           +40 u32 table[5][hubCount], type-major; each u32 packs up to
//!               four entry bytes, LOW byte first, a 0 byte ends the list
//!         BF2, variable:
//!           +0..+39 as BF1
//!           +40 u8  rowCount[5]
//!           +45 for t in 0..5: rowCount[t] rows of hubCount bytes
//!               (row k = the k-th alternative)
//!   ARCS  arcCount records of 22 B (BF1) or 26 B (BF2):
//!           +0 char name[16]; +16 u8 hubA; +17 u8 hubB; +18 u32 typeMask;
//!           BF2 +22 u32 flags (bits 0..2 = a unit cannot be localised on
//!           this arc — FUN_002eb918 / FUN_002ebb48)
//! ```
//!
//! An entry byte is `weight << 3 | slot`: `slot` (0..7) picks one of the hub's
//! eight arc slots and `weight` (0..31) is its share; a 0 byte means "no route"
//! (and is what a hub's own entry holds). The weights of one destination sum
//! to 31 on 1,386,877 of the discs' 1,386,957 entries.
//!
//! [`RouteTable`] stores BOTH games in the BF2 ROW form — row k is the k-th
//! alternative — because that is the shape the chooser wants and it makes the
//! two layouts one type. A BF1 `u32` unpacks into up to four row bytes.
//!
//! **The layout is detected by EXACT TILING, never by game**: a NODE payload of
//! `hubCount * (40 + 20*hubCount)` bytes is the BF1 fixed record; otherwise the
//! BF2 variable records must consume the payload to the byte. Likewise ARCS is
//! `arcCount * 22` or `arcCount * 26`. Anything else is a [`UcfbError`] — this
//! crate's no-panic contract: every truncation is an error, never a panic and
//! never an allocation sized by an unvalidated count.
//!
//! ```text
//! plnp  (BF2 only; always the chunk right after the `plan` it patches)
//!   u8 groupCount (< 9 — the loader's own check)
//!   u8 namesPerGroup[groupCount]
//!   u8 nameCount
//!   u32 arcNameHash[nameCount]   fnv1a of ARC names, grouped consecutively
//!   (1 << groupCount) records: u32 rawSize; u32 zSize; u8 zlib[zSize]
//! ```
//!
//! Record index = the blocked-group bitmask, so record 0 (all groups open) is
//! `hubCount` zero bytes. An inflated record is an XOR patch over the route
//! table (per hub `u8 has`; if set, five `u8 rows` + `rows * hubCount` bytes)
//! — this module keeps the inflated bytes; `core::route_patch` applies them.

use crate::{le, Chunk, UcfbError};
use std::io::Read;

/// One planning hub: a sphere the AI walks between, with up to eight arc slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Hub {
    /// `HubN` as authored. Both loaders read this into a scratch buffer and
    /// throw it away; it is kept here because it is how the RE census and the
    /// mission Lua name hubs.
    pub name: String,
    /// World frame AS STORED (not z-negated — verified by nearest-instance
    /// fit on every map).
    pub pos: [f32; 3],
    pub radius: f32,
    /// Indices into `Plan::arcs`; `0xff` = empty. Prefix-packed on all 5,282
    /// hubs of both discs.
    pub slots: [u8; 8],
}

/// One arc: a corridor between two hubs, walked in both directions.
#[derive(Debug, Clone, PartialEq)]
pub struct Arc {
    /// `ConnectionN` as authored — `plnp` blocks arcs by the fnv1a of this.
    pub name: String,
    pub a: u16,
    pub b: u16,
    /// `AISizeType` mask: bit0 SOLDIER, 1 HOVER, 2 SMALL, 3 MEDIUM, 4 HUGE,
    /// 5 FLYER.
    pub type_mask: u32,
    /// BF2 only (0 on a BF1 arc record, which has no such field).
    pub flags: u32,
}

/// The authored routing tables, in the BF2 row form for both games.
///
/// `rows[type][hub]` is that hub's alternative rows for that unit type: row k
/// is the k-th alternative and holds one entry byte per destination hub, so
/// every row is `hub_count` bytes. Both loaders keep at most four rows per
/// type (a 0x50-byte memset of five four-pointer arrays) and four is the
/// discs' maximum.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteTable {
    pub hub_count: usize,
    pub rows: [Vec<Vec<Vec<u8>>>; 5],
}

impl RouteTable {
    fn empty(hub_count: usize) -> RouteTable {
        RouteTable {
            hub_count,
            rows: std::array::from_fn(|_| Vec::new()),
        }
    }

    /// The alternative rows for `hub` and unit type `t` (0..5), or `&[]` when
    /// either index is out of range.
    pub fn hub_rows(&self, t: usize, hub: usize) -> &[Vec<u8>] {
        self.rows
            .get(t)
            .and_then(|per_hub| per_hub.get(hub))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// One `plan` chunk: the whole authored graph of a world.
#[derive(Debug, Clone)]
pub struct Plan {
    pub hubs: Vec<Hub>,
    pub arcs: Vec<Arc>,
    pub routes: RouteTable,
}

/// `name[16]`, NUL-terminated (or the full 16 bytes when it is not).
fn name16(p: &[u8]) -> String {
    let end = p.iter().position(|&b| b == 0).unwrap_or(p.len());
    String::from_utf8_lossy(&p[..end]).into_owned()
}

/// The 40-byte head every hub record starts with, in both layouts.
fn hub_head(p: &[u8]) -> Hub {
    let f = |o: usize| le::f32_at(p, o);
    let mut slots = [0u8; 8];
    slots.copy_from_slice(&p[32..40]);
    Hub {
        name: name16(&p[0..16]),
        pos: [f(16), f(20), f(24)],
        radius: f(28),
        slots,
    }
}

impl Plan {
    pub fn parse(plan: &Chunk<'_>) -> Result<Plan, UcfbError> {
        let bad = || UcfbError::Truncated(plan.offset);
        let info = plan.child(b"INFO").ok_or_else(bad)?.payload;
        if info.len() < 6 {
            return Err(bad());
        }
        let hub_count = u16::from_le_bytes(info[0..2].try_into().unwrap()) as usize;
        let arc_count = u16::from_le_bytes(info[2..4].try_into().unwrap()) as usize;
        // info[4..6] is typeCount: 5 on every shipped chunk, and BOTH loaders
        // read it and discard it. The five tables are hard-coded.
        let node = plan.child(b"NODE").ok_or_else(bad)?.payload;
        let arcs_payload = plan.child(b"ARCS").ok_or_else(bad)?.payload;

        // --- NODE: pick the layout by exact tiling, never by game ----------
        // BF1's fixed record is 40 + 20*hubCount B. hubCount is a u16 so the
        // product can pass u32; do it in u64 so an absurd count cannot wrap
        // into a match.
        let hc64 = hub_count as u64;
        let fixed_total = 20u64
            .checked_mul(hc64)
            .and_then(|r| r.checked_add(40))
            .and_then(|rec| rec.checked_mul(hc64));
        let (hubs, routes) = if fixed_total == Some(node.len() as u64) {
            parse_node_fixed(node, hub_count)
        } else {
            parse_node_variable(node, hub_count, plan.offset)?
        };

        // --- ARCS: 22 B (BF1) or 26 B (BF2), and it must tile exactly ------
        let stride = if arc_count * 22 == arcs_payload.len() {
            22
        } else if arc_count * 26 == arcs_payload.len() {
            26
        } else {
            return Err(UcfbError::Malformed(format!(
                "plan 0x{:X}: ARCS is {} B, not {arc_count}x22 (BF1) or x26 (BF2)",
                plan.offset,
                arcs_payload.len()
            )));
        };
        let mut arcs = Vec::new();
        for rec in arcs_payload.chunks_exact(stride) {
            arcs.push(Arc {
                name: name16(&rec[0..16]),
                a: u16::from(rec[16]),
                b: u16::from(rec[17]),
                type_mask: u32::from_le_bytes(rec[18..22].try_into().unwrap()),
                flags: if stride == 26 {
                    u32::from_le_bytes(rec[22..26].try_into().unwrap())
                } else {
                    0
                },
            });
        }

        Ok(Plan { hubs, arcs, routes })
    }
}

/// BF1: `hubCount` fixed records of `40 + 20*hubCount` B. The caller has
/// already proven the payload is exactly that many bytes, so no index here can
/// run off the end.
fn parse_node_fixed(node: &[u8], hub_count: usize) -> (Vec<Hub>, RouteTable) {
    let rec = 40 + 20 * hub_count;
    let mut hubs = Vec::new();
    let mut routes = RouteTable::empty(hub_count);
    for i in 0..hub_count {
        let p = &node[i * rec..(i + 1) * rec];
        hubs.push(hub_head(p));
        for (t, per_hub) in routes.rows.iter_mut().enumerate() {
            let base = 40 + t * 4 * hub_count;
            // Unpack each u32 into up to four entry bytes (low byte first, a
            // 0 byte ends the list), then transpose: row k = the k-th
            // alternative over every destination, which is BF2's shape.
            let mut cells: Vec<[u8; 4]> = Vec::with_capacity(hub_count);
            let mut row_count = 0usize;
            for d in 0..hub_count {
                let at = base + 4 * d;
                let mut v = u32::from_le_bytes(p[at..at + 4].try_into().unwrap());
                let mut cell = [0u8; 4];
                let mut k = 0;
                while k < 4 {
                    let b = (v & 0xff) as u8;
                    if b == 0 {
                        break;
                    }
                    cell[k] = b;
                    k += 1;
                    v >>= 8;
                }
                row_count = row_count.max(k);
                cells.push(cell);
            }
            let mut hub_rows = Vec::with_capacity(row_count);
            for k in 0..row_count {
                hub_rows.push(cells.iter().map(|c| c[k]).collect());
            }
            per_hub.push(hub_rows);
        }
    }
    (hubs, routes)
}

/// BF2: variable records, `45 + hubCount * Σ rowCount` B each. Every read is
/// bounds-checked BEFORE it allocates, so a bogus hubCount or rowCount is a
/// `Truncated`, not a panic and not a huge allocation.
fn parse_node_variable(
    node: &[u8],
    hub_count: usize,
    offset: usize,
) -> Result<(Vec<Hub>, RouteTable), UcfbError> {
    let mut hubs = Vec::new();
    let mut routes = RouteTable::empty(hub_count);
    let mut pos = 0usize;
    for _ in 0..hub_count {
        if pos + 45 > node.len() {
            return Err(UcfbError::Truncated(offset));
        }
        hubs.push(hub_head(&node[pos..pos + 40]));
        let row_counts: [u8; 5] = node[pos + 40..pos + 45].try_into().unwrap();
        pos += 45;
        for (t, per_hub) in routes.rows.iter_mut().enumerate() {
            let n = row_counts[t] as usize;
            // n <= 255 and hub_count <= 65535, so this cannot overflow.
            let need = n * hub_count;
            if pos + need > node.len() {
                return Err(UcfbError::Truncated(offset));
            }
            let mut hub_rows = Vec::with_capacity(n);
            for k in 0..n {
                hub_rows.push(node[pos + k * hub_count..pos + (k + 1) * hub_count].to_vec());
            }
            pos += need;
            per_hub.push(hub_rows);
        }
    }
    if pos != node.len() {
        return Err(UcfbError::Malformed(format!(
            "plan 0x{offset:X}: NODE is {} B, the {hub_count} records tile {pos}",
            node.len()
        )));
    }
    Ok((hubs, routes))
}

/// One `plnp` chunk: the XOR patches `BlockPlanningGraphArcs` applies.
#[derive(Debug, Clone)]
pub struct Plnp {
    /// The fnv1a hashes of the ARC NAMES in each blockable group, in order.
    pub groups: Vec<Vec<u32>>,
    /// One INFLATED record per blocked-group bitmask (`1 << groups.len()` of
    /// them), each exactly the declared `rawSize`. Record 0 (nothing blocked)
    /// is `hubCount` zero bytes.
    pub records: Vec<Vec<u8>>,
}

impl Plnp {
    pub fn parse(plnp: &Chunk<'_>) -> Result<Plnp, UcfbError> {
        let p = plnp.payload;
        let at = plnp.offset;
        let bad = || UcfbError::Truncated(at);
        let mut pos = 0usize;
        let byte = |pos: &mut usize| -> Result<u8, UcfbError> {
            let b = *p.get(*pos).ok_or_else(bad)?;
            *pos += 1;
            Ok(b)
        };
        let group_count = byte(&mut pos)? as usize;
        // The loader's own check (FUN_002eb758): a bitmask over the groups
        // must fit a byte, so nine groups or more is not a plnp.
        if group_count >= 9 {
            return Err(UcfbError::Malformed(format!(
                "plnp 0x{at:X}: groupCount {group_count} >= 9"
            )));
        }
        let mut per_group = Vec::with_capacity(group_count);
        for _ in 0..group_count {
            per_group.push(byte(&mut pos)? as usize);
        }
        let name_count = byte(&mut pos)? as usize;
        if per_group.iter().sum::<usize>() != name_count {
            return Err(UcfbError::Malformed(format!(
                "plnp 0x{at:X}: namesPerGroup sums to {}, nameCount is {name_count}",
                per_group.iter().sum::<usize>()
            )));
        }
        let hashes_end = pos + name_count * 4;
        if hashes_end > p.len() {
            return Err(bad());
        }
        let mut hashes = p[pos..hashes_end]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c));
        pos = hashes_end;
        let groups: Vec<Vec<u32>> = per_group
            .iter()
            .map(|&n| hashes.by_ref().take(n).collect())
            .collect();

        let mut records = Vec::new();
        for _ in 0..(1usize << group_count) {
            if pos + 8 > p.len() {
                return Err(bad());
            }
            let raw_size = u32::from_le_bytes(p[pos..pos + 4].try_into().unwrap()) as u64;
            let z_size = u32::from_le_bytes(p[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            let end = pos.checked_add(z_size).ok_or_else(bad)?;
            if end > p.len() {
                return Err(bad());
            }
            // Inflate with a HARD cap of the declared rawSize (+1 byte, so an
            // over-long stream is caught rather than silently truncated): the
            // zlib payload is disc data and nothing else bounds it.
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&p[pos..end])
                .take(raw_size + 1)
                .read_to_end(&mut out)
                .map_err(|e| UcfbError::Malformed(format!("plnp 0x{at:X}: inflate: {e}")))?;
            if out.len() as u64 != raw_size {
                return Err(UcfbError::Malformed(format!(
                    "plnp 0x{at:X}: record inflates to {} B, rawSize says {raw_size}",
                    out.len()
                )));
            }
            records.push(out);
            pos = end;
        }
        if pos != p.len() {
            return Err(UcfbError::Malformed(format!(
                "plnp 0x{at:X}: {} B payload, records tile {pos}",
                p.len()
            )));
        }
        Ok(Plnp { groups, records })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_chunk;

    fn first(buf: &[u8]) -> Chunk<'_> {
        crate::chunks(buf).next().unwrap().unwrap()
    }

    fn hub_bytes(name: &str, pos: [f32; 3], radius: f32, slots: [u8; 8]) -> Vec<u8> {
        let mut v = vec![0u8; 16];
        v[..name.len()].copy_from_slice(name.as_bytes());
        for c in pos {
            v.extend(c.to_le_bytes());
        }
        v.extend(radius.to_le_bytes());
        v.extend(slots);
        v
    }

    fn arc_bytes(name: &str, a: u8, b: u8, mask: u32, flags: Option<u32>) -> Vec<u8> {
        let mut v = vec![0u8; 16];
        v[..name.len()].copy_from_slice(name.as_bytes());
        v.push(a);
        v.push(b);
        v.extend(mask.to_le_bytes());
        if let Some(f) = flags {
            v.extend(f.to_le_bytes());
        }
        v
    }

    fn plan_chunk(hub_count: u16, arc_count: u16, node: &[u8], arcs: &[u8]) -> Vec<u8> {
        let mut info = Vec::new();
        info.extend(hub_count.to_le_bytes());
        info.extend(arc_count.to_le_bytes());
        info.extend(5u16.to_le_bytes());
        let mut body = build_chunk(b"INFO", &info);
        body.extend(build_chunk(b"NODE", node));
        body.extend(build_chunk(b"ARCS", arcs));
        build_chunk(b"plan", &body)
    }

    /// Two hubs in the BF1 fixed layout: the u32 table unpacks into rows.
    #[test]
    fn synthetic_bf1_fixed_node_unpacks_into_rows() {
        let mut node = Vec::new();
        for (i, slots) in [[0u8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]; 2]
            .into_iter()
            .enumerate()
        {
            node.extend(hub_bytes(
                &format!("Hub{i}"),
                [i as f32, 1.0, 2.0],
                3.0,
                slots,
            ));
            // table[5][2]; type 0 holds two alternatives for dest 1.
            for t in 0..5u32 {
                for d in 0..2u32 {
                    let v: u32 = if t == 0 && d == 1 {
                        0x0000_49f9 // (31, 1) then (9, 1)
                    } else {
                        0
                    };
                    node.extend(v.to_le_bytes());
                }
            }
        }
        let arcs = arc_bytes("Connection1", 0, 1, 1, None);
        let buf = plan_chunk(2, 1, &node, &arcs);
        let p = Plan::parse(&first(&buf)).unwrap();
        assert_eq!(p.hubs.len(), 2);
        assert_eq!(p.hubs[1].name, "Hub1");
        assert_eq!(
            p.hubs[0].slots,
            [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
        assert_eq!(p.routes.hub_count, 2);
        // Two alternatives on type 0, none on the other four.
        assert_eq!(p.routes.hub_rows(0, 0), &[vec![0u8, 0xf9], vec![0u8, 0x49]]);
        for t in 1..5 {
            assert!(p.routes.hub_rows(t, 0).is_empty());
        }
        assert_eq!(p.arcs.len(), 1);
        assert_eq!(p.arcs[0].name, "Connection1");
        assert_eq!((p.arcs[0].a, p.arcs[0].b, p.arcs[0].flags), (0, 1, 0));
    }

    /// The same graph in the BF2 variable layout, with 26-byte arcs.
    #[test]
    fn synthetic_bf2_variable_node_and_flags() {
        let mut node = Vec::new();
        for i in 0..2 {
            node.extend(hub_bytes(&format!("Hub{i}"), [0.0; 3], 1.0, [0xff; 8]));
            node.extend([2u8, 0, 0, 0, 0]); // rowCount[5]
            node.extend([0u8, 0xf9]); // type 0 row 0
            node.extend([0u8, 0x49]); // type 0 row 1
        }
        let arcs = arc_bytes("Connection1", 0, 1, 7, Some(3));
        let buf = plan_chunk(2, 1, &node, &arcs);
        let p = Plan::parse(&first(&buf)).unwrap();
        assert_eq!(p.routes.hub_rows(0, 1), &[vec![0u8, 0xf9], vec![0u8, 0x49]]);
        assert_eq!(p.arcs[0].type_mask, 7);
        assert_eq!(p.arcs[0].flags, 3);
    }

    /// hubCount 0 with empty NODE/ARCS is a real shipped chunk (BF2 KAM1
    /// @0x4DA88) and must parse, not error.
    #[test]
    fn empty_plan_is_accepted() {
        let buf = plan_chunk(0, 0, &[], &[]);
        let p = Plan::parse(&first(&buf)).unwrap();
        assert!(p.hubs.is_empty() && p.arcs.is_empty());
        assert_eq!(p.routes.hub_count, 0);
    }

    #[test]
    fn truncated_node_is_an_error_not_a_panic() {
        // Two hubs promised, one 45-byte record present: tiles neither layout.
        let mut node = hub_bytes("Hub0", [0.0; 3], 1.0, [0xff; 8]);
        node.extend([0u8; 5]);
        let buf = plan_chunk(2, 0, &node, &[]);
        assert!(matches!(
            Plan::parse(&first(&buf)),
            Err(UcfbError::Truncated(_))
        ));
    }

    #[test]
    fn node_that_tiles_neither_layout_is_an_error() {
        // One hub, BF2 record plus one stray byte.
        let mut node = hub_bytes("Hub0", [0.0; 3], 1.0, [0xff; 8]);
        node.extend([0u8; 5]);
        node.push(0);
        let buf = plan_chunk(1, 0, &node, &[]);
        assert!(matches!(
            Plan::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    #[test]
    fn absurd_hub_count_is_an_error_not_an_allocation() {
        // 60,000 hubs promised over a 45-byte NODE: neither layout tiles, and
        // the variable walk must bail on the FIRST record rather than size
        // anything by the count.
        let mut node = hub_bytes("Hub0", [0.0; 3], 1.0, [0xff; 8]);
        node.extend([0xffu8; 5]);
        let buf = plan_chunk(60_000, 0, &node, &[]);
        assert!(matches!(
            Plan::parse(&first(&buf)),
            Err(UcfbError::Truncated(_))
        ));
    }

    #[test]
    fn arcs_that_tile_neither_stride_is_an_error() {
        let mut node = hub_bytes("Hub0", [0.0; 3], 1.0, [0xff; 8]);
        node.extend([0u8; 5]);
        let arcs = vec![0u8; 24]; // neither 22 nor 26
        let buf = plan_chunk(1, 1, &node, &arcs);
        assert!(matches!(
            Plan::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    #[test]
    fn missing_children_are_errors() {
        let buf = build_chunk(b"plan", &build_chunk(b"INFO", &[0u8; 6]));
        assert!(matches!(
            Plan::parse(&first(&buf)),
            Err(UcfbError::Truncated(_))
        ));
    }

    fn deflate(raw: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(raw).unwrap();
        e.finish().unwrap()
    }

    fn plnp_chunk(per_group: &[u8], hashes: &[u32], records: &[Vec<u8>]) -> Vec<u8> {
        let mut p = vec![per_group.len() as u8];
        p.extend(per_group);
        p.push(hashes.len() as u8);
        for h in hashes {
            p.extend(h.to_le_bytes());
        }
        for r in records {
            let z = deflate(r);
            p.extend((r.len() as u32).to_le_bytes());
            p.extend((z.len() as u32).to_le_bytes());
            p.extend(z);
        }
        build_chunk(b"plnp", &p)
    }

    #[test]
    fn synthetic_plnp_groups_and_inflates() {
        let recs = vec![vec![0u8; 4], vec![1u8, 2, 3, 4]];
        let buf = plnp_chunk(&[2], &[0xdead_beef, 0x0bad_f00d], &recs);
        let p = Plnp::parse(&first(&buf)).unwrap();
        assert_eq!(p.groups, vec![vec![0xdead_beef, 0x0bad_f00d]]);
        assert_eq!(p.records, recs);
    }

    #[test]
    fn plnp_with_too_many_groups_is_an_error() {
        let mut p = vec![9u8];
        p.extend([1u8; 9]);
        p.push(9);
        let buf = build_chunk(b"plnp", &p);
        assert!(matches!(
            Plnp::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    #[test]
    fn truncated_plnp_is_an_error_not_a_panic() {
        // Header promising two records, with the second cut off.
        let mut full = plnp_chunk(&[1], &[7], &[vec![0u8; 3], vec![9u8; 3]]);
        full.truncate(full.len() - 6);
        let payload_len = full.len() - 8;
        full[4..8].copy_from_slice(&(payload_len as u32).to_le_bytes());
        assert!(Plnp::parse(&first(&full)).is_err());
    }

    #[test]
    fn plnp_record_that_lies_about_its_raw_size_is_an_error() {
        let mut buf = plnp_chunk(&[1], &[7], &[vec![0u8; 8], vec![0u8; 8]]);
        // Rewrite the first record's rawSize (payload offset 1+1+1+4 = 7).
        let at = 8 + 7;
        buf[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
        assert!(matches!(
            Plnp::parse(&first(&buf)),
            Err(UcfbError::Malformed(_))
        ));
    }

    /// Pinned vectors measured on the raw discs (RE_LOG 2026-09-01
    /// `plan`/`plnp`/`Hint`/`BARR`). Data-gated: skipped when the discs are
    /// not extracted locally.
    fn disc(rel: &str) -> Option<Vec<u8>> {
        let path = format!("{}/../../../extracted/{rel}", env!("CARGO_MANIFEST_DIR"));
        if !std::path::Path::new(&path).exists() {
            eprintln!("SKIP: {path} not present");
            return None;
        }
        Some(std::fs::read(path).unwrap())
    }

    /// Find the top-level chunk whose FILE offset is `file_offset`.
    fn chunk_at(inner: &[u8], file_offset: usize) -> Option<Chunk<'_>> {
        crate::chunks(inner)
            .flatten()
            .find(|c| c.offset + 8 == file_offset)
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn tat1_pinned_plan_vectors() {
        let Some(buf) = disc("bf1/lvl/DATA/_LVL_PS2/TAT/TAT1.LVL") else {
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let c = chunk_at(inner, 0x2D0F0).expect("plan at 0x2D0F0");
        assert_eq!(&c.tag, b"plan");
        // INFO payload @0x2D100 = 73 00 97 00 05 00
        let info = c.child(b"INFO").unwrap().payload;
        assert_eq!(&info[..6], &[0x73, 0x00, 0x97, 0x00, 0x05, 0x00]);
        let p = Plan::parse(&c).unwrap();
        assert_eq!((p.hubs.len(), p.arcs.len()), (115, 151));

        // hub 0 "Hub39": no arcs, no routes.
        let h0 = &p.hubs[0];
        assert_eq!(h0.name, "Hub39");
        assert!(near(h0.pos[0], -1093.6193), "{:?}", h0.pos);
        assert!(near(h0.pos[1], -2921.0146), "{:?}", h0.pos);
        assert!(near(h0.pos[2], -699.7124), "{:?}", h0.pos);
        assert!(near(h0.radius, 94.165_63), "{}", h0.radius);
        assert_eq!(h0.slots, [0xff; 8]);
        for t in 0..5 {
            assert!(p.routes.hub_rows(t, 0).is_empty(), "hub 0 type {t}");
        }

        // hub 2 "Hub33" @0x2E358 — three arcs, a routed table.
        let h2 = &p.hubs[2];
        assert_eq!(h2.name, "Hub33");
        assert!(near(h2.pos[0], -438.51852), "{:?}", h2.pos);
        assert!(near(h2.pos[1], 20.513742), "{:?}", h2.pos);
        assert!(near(h2.pos[2], 115.34089), "{:?}", h2.pos);
        assert!(near(h2.radius, 8.0), "{}", h2.radius);
        assert_eq!(h2.slots, [80, 93, 132, 0xff, 0xff, 0xff, 0xff, 0xff]);
        // table[0][3] = 0x000000f8 -> one alternative, weight 31 slot 0.
        let t0 = p.routes.hub_rows(0, 2);
        assert_eq!(t0.len(), 1);
        assert_eq!(t0[0][3], 0xf8);
        assert_eq!(t0[0][3] >> 3, 31);
        assert_eq!(t0[0][3] & 7, 0);
        assert_eq!(t0[0].iter().filter(|&&b| b != 0).count(), 111);
        assert_eq!(t0[0].len(), 115);
        let t1 = p.routes.hub_rows(1, 2);
        assert_eq!(t1[0].iter().filter(|&&b| b != 0).count(), 63);

        // ARCS payload @0x6EC44 (so the ARCS chunk header is 8 B earlier).
        let arcs = c.child(b"ARCS").unwrap();
        assert_eq!(arcs.offset + 16, 0x6EC44);
        assert_eq!(
            &arcs.payload[..22],
            &[
                0x43, 0x6f, 0x6e, 0x6e, 0x65, 0x63, 0x74, 0x69, 0x6f, 0x6e, 0x36, 0x33, 0x00, 0x00,
                0x00, 0x00, 0x03, 0x05, 0x01, 0x00, 0x00, 0x00
            ]
        );
        assert_eq!(p.arcs[0].name, "Connection63");
        assert_eq!((p.arcs[0].a, p.arcs[0].b), (3, 5));
        assert_eq!(p.arcs[0].type_mask, 1);
        assert_eq!(p.arcs[0].flags, 0); // BF1 arcs have no flags field
        assert_eq!(p.arcs[1].name, "Connection64");
        assert_eq!((p.arcs[1].a, p.arcs[1].b), (5, 6));
        assert_eq!(p.arcs[1].type_mask, 1);
    }

    #[test]
    fn tat2_pinned_plan_vectors() {
        let Some(buf) = disc("bf2/lvl/DATA/_LVL_PS2/TAT/TAT2.LVL") else {
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let c = chunk_at(inner, 0x14914).expect("plan at 0x14914");
        let info = c.child(b"INFO").unwrap().payload;
        assert_eq!(&info[..6], &[0xc8, 0x00, 0xf2, 0x00, 0x05, 0x00]);
        let p = Plan::parse(&c).unwrap();
        assert_eq!((p.hubs.len(), p.arcs.len()), (200, 242));

        let h0 = &p.hubs[0];
        assert_eq!(h0.name, "Hub0");
        assert!(near(h0.pos[0], 132.69946), "{:?}", h0.pos);
        assert!(near(h0.pos[1], 0.001129), "{:?}", h0.pos);
        assert!(near(h0.pos[2], 11.74349), "{:?}", h0.pos);
        assert!(near(h0.radius, 5.701_843), "{}", h0.radius);
        assert_eq!(h0.slots, [2, 3, 176, 0xff, 0xff, 0xff, 0xff, 0xff]);
        // rows [2, 1, 1, 0, 0]
        let counts: Vec<usize> = (0..5).map(|t| p.routes.hub_rows(t, 0).len()).collect();
        assert_eq!(counts, vec![2, 1, 1, 0, 0]);
        let t0 = p.routes.hub_rows(0, 0);
        assert_eq!(t0[0].len(), 200);
        assert_eq!(t0[0].iter().filter(|&&b| b != 0).count(), 199);
        assert_eq!(t0[0][1], 0xf9); // weight 31, slot 1
        assert_eq!((t0[0][1] >> 3, t0[0][1] & 7), (31, 1));
        // row 1 routes only two destinations.
        let set: Vec<(usize, u8)> = t0[1]
            .iter()
            .enumerate()
            .filter(|(_, &b)| b != 0)
            .map(|(d, &b)| (d, b))
            .collect();
        assert_eq!(set, vec![(23, 0x49), (149, 0x7a)]);
        assert_eq!((0x49u8 >> 3, 0x49u8 & 7), (9, 1));
        assert_eq!((0x7au8 >> 3, 0x7au8 & 7), (15, 2));

        let h1 = &p.hubs[1];
        assert_eq!(h1.name, "Hub1");
        assert!(near(h1.pos[0], 108.8884), "{:?}", h1.pos);
        assert!(near(h1.pos[1], 0.001125), "{:?}", h1.pos);
        assert!(near(h1.pos[2], 12.022558), "{:?}", h1.pos);
        assert!(near(h1.radius, 5.705001), "{}", h1.radius);
        assert_eq!(h1.slots, [3, 4, 229, 230, 0xff, 0xff, 0xff, 0xff]);
        let counts: Vec<usize> = (0..5).map(|t| p.routes.hub_rows(t, 1).len()).collect();
        assert_eq!(counts, vec![1, 1, 1, 0, 0]);

        let arcs = c.child(b"ARCS").unwrap();
        assert_eq!(arcs.offset + 16, 0x35574);
        assert_eq!(
            &arcs.payload[..26],
            &[
                0x43, 0x6f, 0x6e, 0x6e, 0x65, 0x63, 0x74, 0x69, 0x6f, 0x6e, 0x31, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x06, 0x07, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
        assert_eq!(p.arcs[0].name, "Connection1");
        assert_eq!((p.arcs[0].a, p.arcs[0].b), (6, 7));
        assert_eq!((p.arcs[0].type_mask, p.arcs[0].flags), (7, 0));
        assert_eq!(p.arcs[1].name, "Connection5");
        assert_eq!((p.arcs[1].a, p.arcs[1].b), (9, 8));
        assert_eq!(p.arcs[1].type_mask, 7);
    }

    #[test]
    fn kas2_pinned_plnp_header_and_record_zero() {
        let Some(buf) = disc("bf2/lvl/DATA/_LVL_PS2/KAS/KAS2.LVL") else {
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let c = chunk_at(inner, 0x434A8).expect("plnp at 0x434A8");
        assert_eq!(&c.tag, b"plnp");
        assert_eq!(c.payload.len(), 5626);
        // 04 | 01 01 01 01 | 04 | eb a5 db 6d ... (fnv1a("seawall1") first)
        assert_eq!(
            &c.payload[..10],
            &[0x04, 0x01, 0x01, 0x01, 0x01, 0x04, 0xeb, 0xa5, 0xdb, 0x6d]
        );
        let p = Plnp::parse(&c).unwrap();
        assert_eq!(p.groups.len(), 4);
        assert!(p.groups.iter().all(|g| g.len() == 1));
        assert_eq!(p.groups[0][0], 0x6ddb_a5eb);
        assert_eq!(p.groups[0][0], crate::fnv1a_lowercase("seawall1"));
        assert_eq!(p.records.len(), 16);
        // Record 0 = nothing blocked = hubCount (59) zero bytes.
        assert_eq!(p.records[0].len(), 59);
        assert!(p.records[0].iter().all(|&b| b == 0));
        // ...and 59 is this map's hubCount.
        let plan = chunk_at(inner, 0x3F6AC).expect("plan at 0x3F6AC");
        assert_eq!(Plan::parse(&plan).unwrap().hubs.len(), 59);
    }

    #[test]
    fn pol1_pinned_plnp_record_sizes() {
        let Some(buf) = disc("bf2/lvl/DATA/_LVL_PS2/POL/POL1.LVL") else {
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let c = chunk_at(inner, 0x367AC).expect("plnp at 0x367AC");
        assert_eq!(c.payload.len(), 1786);
        let p = Plnp::parse(&c).unwrap();
        // One group of two arc names (Connection110, Connection264).
        assert_eq!(p.groups.len(), 1);
        assert_eq!(p.groups[0].len(), 2);
        assert_eq!(p.groups[0][0], crate::fnv1a_lowercase("Connection110"));
        assert_eq!(p.groups[0][1], crate::fnv1a_lowercase("Connection264"));
        assert_eq!(p.records.len(), 2);
        assert_eq!(p.records[0].len(), 189);
        assert_eq!(p.records[1].len(), 72745);
        // Record 0 is hubCount (189) zero bytes.
        assert!(p.records[0].iter().all(|&b| b == 0));
    }
}
