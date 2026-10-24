//! `tern -> WATR`: a map's water surface — where it is, how high, and what it
//! is painted with. Decoded 2026-08-31 (Opus session 4); A2 had `WATR` listed
//! but undecoded, and no map drew water.
//!
//! Present on **12 of the 34 ground maps**: BF1 KAM1/KAS1/KAS2/NAB2/YAV1 and
//! BF2 DAG1/DEA1/FEL1/KAM1/KAS2/NAB2/YAV1. It lives under `tern` even when
//! that `tern` is a heightfield STUB — on BF2 DEA1 and BF2 KAM1 the `tern`
//! carries nothing but `NAME` + `WATR`, so **the water plane is the only
//! ground those two maps have**. Parse it independently of the heightfield.
//!
//! ```text
//! tern -> WATR -> INFO (24 B)   grid, 2, cellSize, halfExtentX, halfExtentZ, n
//!              -> LAYR          cstring texture, u32 10, 5 f32, 4 B colour
//!              -> WMAP          grid*grid/2 B — one NIBBLE per cell
//!              -> CPAT          0 B on 11 of 14; else LPTC x n
//! ```
//!
//! ## `INFO` — the cell grid
//!
//! Six words: `u32 grid`, `u32` (**2** on all 14 shipped chunks), `f32
//! cellSize`, `f32 halfExtentX`, `f32 halfExtentZ`, `u32` (1 on twelve, 4 on
//! the two Kashyyyk Docks). MEASURED on all 14: `grid * cellSize` is exactly
//! `2 * halfExtent` every time, so the last two floats are HALF-extents and
//! the sheet is centred on the world origin — Kamino 128 cells of 160 m over
//! ±10240 m, Kashyyyk 1 64 of 32 m over ±1024, Dagobah 128 of 16 m over
//! ±1024, Felucia 64 of 16 m over ±512.
//!
//! The water sheet is NOT the same size as the terrain: it matches on BF1
//! KAS1/NAB2/YAV1, is 2x the terrain on BF1 KAM1 and BF2 FEL1, and 4x on
//! BF2 DAG1. Water runs past the edge of the heightfield.
//!
//! ## `WMAP` — one nibble per cell, and it is a boolean
//!
//! `grid * grid / 2` bytes exactly on all 14. Read as nibbles the values are
//! only ever **0, 1 or 9** (`9` on BF1 NAB2, BF2 NAB2 and BF2 DEA1, 1-8 cells
//! each, and always on the SHORELINE ring of a body of water). Bit 0 is
//! "this cell has water".
//!
//! **Bit 3 is DECODED (2026-09-09, `analysis/re-water.md` §4, [V]): it means
//! "an `LPTC` for this cell follows in `CPAT`".** The parser walks `CPAT` once
//! per bit-3 cell in row-major order (BF1 `FUN_00475e08` 538764-538787, BF2
//! 696379-696396), and on all 14 shipped chunks the bit-3 cell count equals
//! the `LPTC` count exactly (BF1 NAB2 4 = 4, BF2 NAB2 8 = 8, BF2 DEA1 1 = 1,
//! eleven 0 = 0) — pinned by `bit_three_cells_match_the_patch_count` in
//! `tests/suite/fx_lvl.rs`. See [`Water::has_patch`] and [`Water::patch_cells`].
//!
//! The PS2's own wet test is `nibble != 0` (539348-539356) where this reads
//! bit 0. The two agree on every shipped chunk, because no nibble is ever 8
//! without also being 9.
//!
//! Layout is row-major `[z][x]` with the HIGH nibble first. Both were
//! measured, not assumed, by sampling the terrain height under each cell and
//! asking whether it lies below the water height: over the 12 maps, row-major
//! separates wet from dry by **+63 points** of "terrain below the waterline"
//! against **+13** for the transposed reading, and high-nibble-first beats
//! low-nibble-first on 6 maps and loses on none. On the maps with a big body
//! of water it is unambiguous — BF1 KAS1 98.7 % of wet cells below the
//! waterline against 0.0 % of dry, BF2 NAB2 97.8 % against 1.7 %, BF1 KAS2
//! 89.8 % against 0.0 % — and plotted, the mask draws Kashyyyk's island,
//! Theed's city platform, Felucia's river and Yavin's pond exactly.
//!
//! ## `LAYR` — the surface, and the one field that is proven
//!
//! `[cstring texture][u32][f32 height][f32 x2][f32 x2][u8 x4]`, 28 bytes after
//! the name on all 14.
//!
//! **`height` is the world Y of the water plane, and that is proven twice**:
//! by the waterline separation above, and by `CPAT` — every one of the 165
//! shipped water-patch vertices sits at exactly this Y, on all three maps that
//! ship patches, even though the patch bounding box allows a 1 m range.
//!
//! **Everything else in `LAYR` is DEAD DATA — read and dropped by both ELFs**
//! (2026-09-09, `analysis/re-water.md` §1/§3, [V]: BF1 `FUN_00475e08` keeps
//! only the height at 538743 and drops the u32, both f32 pairs and the colour
//! word at 538742 and 538744-538751; BF2 696356-696364 is the twin). It is
//! ZeroEditor's copy of the real config, which lives in the LVL's `fx__`
//! chunk — [`crate::fx::WaterConfig`]. The fields stay parsed and recorded
//! because they are what the bytes hold, and nothing may consume them:
//! - the `u32` is **10 on all 14**;
//! - `uv_velocity` and `uv_tile` are a stale copy of `fx__`'s `Velocity` and
//!   `Tile`, and they disagree with it: BF1 KAS2 stores `LAYR` (1,1) against
//!   `Tile(2,2)` with `INFO5` 4, an 8x difference in the texture scale. The
//!   `fx__` pair is the PS2's;
//! - `color` is four bytes whose byte order the shipped art refutes either
//!   way (RGBA turns Felucia's teal river red-black, BGRA turns Kamino's
//!   ocean red) — and the question is now MOOT, because neither ELF reads it.
//!   The PS2's tint is `fx__`'s Border/Min/MaxDiffuseColor LUT (§3). See
//!   [`WaterLayer::color`].
//!
//! The texture name is EMPTY on 5 of the 14 (both Kashyyyk 2s, BF1 KAS1, BF2
//! NAB2), and on the other nine it is the wrong one anyway: both ELFs read it
//! into a dead 256-byte stack buffer (538741) and paint the surface with
//! `fx__`'s `MainTexture`. On both KAM1s those differ — `LAYR` names the FOAM
//! texture `kam1_water` and `MainTexture` names `kam1_water_2`.
//!
//! ## `CPAT` — the shoreline-fitted surface, on three maps
//!
//! Zero-length on 11 of 14. BF1 NAB2 ships 4 patches, BF2 NAB2 8, BF2 DEA1 1.
//! Each `LPTC` is `INFO` (6 f32 = an AABB as xmin,xmax,ymin,ymax,zmin,zmax),
//! `STRP` (the standard PS2 strip format — see [`crate::model::unstitch_strips`])
//! and `POSI` (3 x u16 dequantised into that AABB, the same convention `segm`
//! uses). **The box is the patch's own cell padded by half a cell on every
//! side** — two cells wide, 64 m over Theed's 32 m cells and 32 m over BF2
//! NAB2's 16 m ones — so its edges run through the eight neighbours' centres
//! and it must never be used to decide which cells a patch owns
//! ([`Water::has_patch`] is the disc's own answer). The triangles cover
//! **13–96 % of the one cell the patch replaces**, which is 3.4–24.0 % of
//! that box — the open water left after the buildings are cut out (measured
//! on all 13 shipped patches, `bake/tests/suite/water_patch_cells.rs`). Every
//! vertex is at the water height.
//!
//! **A patch REPLACES its cell's flat quad — VERIFIED** (2026-09-09,
//! `analysis/re-water.md` §4, [V], lead-reproduced on the BF1 `CPAT` tile
//! loop 551346-551359): the per-frame cull hangs the patch packet on the tile
//! record at `+0x24` (539306-539324) — on an EXACT `z * grid + x` match, never
//! by the patch's box ([`Water::has_patch`]) — and the flat-water draw takes
//! the patch iff that pointer is set (551350-551359, 551383-551392,
//! 551499-551508). The bake's rule was ours and PROVISIONAL; it is now the
//! PS2's. HOLD 10(f) is closed. §4's CORRECTION (2026-09-17) carries the two
//! refinements: the ELSE branch has a middle case before the flat template,
//! and the OCEAN path (`FUN_0048b280`) never reads `+0x24` at all — no shipped
//! ocean map ships a `CPAT`, so neither reaches this parser's callers.

use crate::model::unstitch_strips;
use crate::{le, Chunk, UcfbError};

/// Largest accepted `WMAP` grid. The largest shipped is 128; this is 8x past
/// it, and `grid` is a raw u32 that squares into an allocation below.
const MAX_GRID: u32 = 1024;

/// Bit 0 of a `WMAP` nibble: this cell carries water.
const CELL_WET: u8 = 1;

/// Bit 3 of a `WMAP` nibble: an `LPTC` for this cell follows in `CPAT`
/// (`re-water.md` §4). See the module header.
const CELL_HAS_PATCH: u8 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct WaterInfo {
    /// `WMAP` is `grid x grid` nibbles.
    pub grid: u32,
    /// Metres per `WMAP` cell. `grid * cell_size == 2 * half_extent`.
    pub cell_size: f32,
    /// World half-extent in x and z; the sheet is centred on the origin.
    pub half_extent: [f32; 2],
    /// `INFO[1]` — **2** on all 14 shipped chunks. Undecoded.
    pub unknown_1: u32,
    /// `INFO[5]` — 1 on twelve, 4 on both Kashyyyk Docks. **The cell-size
    /// multiplier the `fx__` config is scaled by** (`re-water.md` §3, [V],
    /// finalise at 538831-538838): `Tile * INFO5`, `Velocity / INFO5`,
    /// `PatchDivisions * INFO5`. See [`crate::fx::WaterConfig`]. Kept under
    /// its raw name because it is still `INFO`'s sixth word and nothing but
    /// those three products is known about it.
    pub unknown_5: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaterLayer {
    /// Texture name, or empty on 5 of the 14 (see the module header).
    pub texture: String,
    /// **10 on all 14 shipped chunks.** Undecoded.
    pub unknown: u32,
    /// World Y of the water plane. PROVEN — see the module header.
    pub height: f32,
    /// Dead data (module header: neither ELF keeps it). By its values, UV
    /// units per second.
    pub uv_velocity: [f32; 2],
    /// Dead data (module header). By its values, texture repeats across the
    /// sheet: always a matched pair of 1.0 or 2.0.
    pub uv_tile: [f32; 2],
    /// Dead data (module header): four bytes, stored here in file order,
    /// whose channel order the art refutes either way. The alpha byte does not fit a
    /// plain 0..255 opacity: BF1 KAS2 stores 0x3C and BF1 NAB2 stores 0x00,
    /// and BF1 NAB2's canals are water on screen in the original. Callers
    /// that want an opacity should not take this byte at face value.
    pub color: [u8; 4],
}

/// One `CPAT -> LPTC`: an exact water surface for a single tile.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterPatch {
    /// `(min, max)` in world space; `min[1] == max[1] - 1.0` on every shipped
    /// patch, and every vertex sits at `min[1]`.
    pub aabb: ([f32; 3], [f32; 3]),
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Water {
    pub info: WaterInfo,
    pub layer: WaterLayer,
    /// `grid * grid` nibbles, row-major `[z][x]`. Use [`Water::is_wet`].
    pub cells: Vec<u8>,
    /// Empty on 11 of the 14 shipped chunks.
    pub patches: Vec<WaterPatch>,
}

impl Water {
    /// Parse the `WATR` under a `tern`. `Ok(None)` when the map ships none
    /// (22 of the 34 ground maps) or the chunk is a bare stub.
    pub fn parse(tern: &Chunk<'_>) -> Result<Option<Water>, UcfbError> {
        let Some(watr) = tern.child(b"WATR") else {
            return Ok(None);
        };
        let bad = || UcfbError::Truncated(watr.offset);
        let (Some(info_c), Some(layr_c), Some(wmap_c)) = (
            watr.child(b"INFO"),
            watr.child(b"LAYR"),
            watr.child(b"WMAP"),
        ) else {
            return Ok(None);
        };

        let ip = info_c.payload;
        if ip.len() < 24 {
            return Err(bad());
        }
        let grid = le::u32_at(ip, 0);
        if grid == 0 || !grid.is_multiple_of(2) || grid > MAX_GRID {
            return Err(UcfbError::Malformed(format!("WATR grid size {grid}")));
        }
        let info = WaterInfo {
            grid,
            cell_size: le::f32_at(ip, 8),
            half_extent: [le::f32_at(ip, 12), le::f32_at(ip, 16)],
            unknown_1: le::u32_at(ip, 4),
            unknown_5: le::u32_at(ip, 20),
        };

        let lp = layr_c.payload;
        let nul = lp.iter().position(|&b| b == 0).ok_or_else(bad)?;
        let texture = std::str::from_utf8(&lp[..nul])
            .map_err(|_| UcfbError::Malformed("WATR LAYR texture name".into()))?
            .to_owned();
        let rest = &lp[nul + 1..];
        if rest.len() < 28 {
            return Err(bad());
        }
        let layer = WaterLayer {
            texture,
            unknown: le::u32_at(rest, 0),
            height: le::f32_at(rest, 4),
            uv_velocity: [le::f32_at(rest, 8), le::f32_at(rest, 12)],
            uv_tile: [le::f32_at(rest, 16), le::f32_at(rest, 20)],
            color: [rest[24], rest[25], rest[26], rest[27]],
        };

        // WMAP: grid*grid nibbles, high nibble first, row-major [z][x].
        let n = (grid as usize) * (grid as usize);
        if wmap_c.payload.len() < n / 2 {
            return Err(bad());
        }
        let mut cells = Vec::with_capacity(n);
        for &b in &wmap_c.payload[..n / 2] {
            cells.push(b >> 4);
            cells.push(b & 0x0F);
        }

        let mut patches = Vec::new();
        if let Some(cpat) = watr.child(b"CPAT") {
            for lptc in cpat.children_tagged(b"LPTC") {
                if let Some(p) = parse_patch(&lptc)? {
                    patches.push(p);
                }
            }
        }

        Ok(Some(Water {
            info,
            layer,
            cells,
            patches,
        }))
    }

    /// Does the cell at `[z][x]` carry water? Out-of-range is dry.
    pub fn is_wet(&self, x: usize, z: usize) -> bool {
        let g = self.info.grid as usize;
        x < g && z < g && self.cells[z * g + x] & CELL_WET != 0
    }

    /// Does the cell at `[z][x]` carry a `CPAT` patch? `WMAP` bit 3 is the
    /// disc's own answer, and the PS2 matches a patch to a cell by exact
    /// index: the per-frame cull hangs the patch packet on `tile+0x24` only
    /// when `z * grid + x` is equal (BF1 `FUN_004770a8` 539306-539324, BF2
    /// `FUN_00561b58` 696884-696892 — lead-re-read 2026-09-17), never by the
    /// patch's bounding box, which is its cell padded by half a cell on every
    /// side. Out-of-range is false.
    ///
    /// The parser reads every `LPTC` under `CPAT` without pairing it to a
    /// cell, so this and `patches.len()` are two counts of the same thing;
    /// they are equal on all 14 shipped chunks ([`Water::patch_cells`]).
    pub fn has_patch(&self, x: usize, z: usize) -> bool {
        let g = self.info.grid as usize;
        x < g && z < g && self.cells[z * g + x] & CELL_HAS_PATCH != 0
    }

    /// World-space min corner (x, z) of cell `[z][x]`.
    pub fn cell_origin(&self, x: usize, z: usize) -> [f32; 2] {
        [
            x as f32 * self.info.cell_size - self.info.half_extent[0],
            z as f32 * self.info.cell_size - self.info.half_extent[1],
        ]
    }

    pub fn wet_cells(&self) -> usize {
        self.cells.iter().filter(|&&c| c & CELL_WET != 0).count()
    }

    /// Cells whose nibble sets bit 3 — i.e. cells the disc says carry a
    /// `CPAT -> LPTC`. Equals `patches.len()` on all 14 shipped chunks
    /// (`re-water.md` §4); the equality is what pins the bit's meaning.
    pub fn patch_cells(&self) -> usize {
        self.cells
            .iter()
            .filter(|&&c| c & CELL_HAS_PATCH != 0)
            .count()
    }
}

fn parse_patch(lptc: &Chunk<'_>) -> Result<Option<WaterPatch>, UcfbError> {
    let bad = || UcfbError::Truncated(lptc.offset);
    let (Some(info), Some(posi), Some(strp)) = (
        lptc.child(b"INFO"),
        lptc.child(b"POSI"),
        lptc.child(b"STRP"),
    ) else {
        return Ok(None);
    };
    if info.payload.len() < 24 {
        return Err(bad());
    }
    let f = |o: usize| le::f32_at(info.payload, o);
    // INFO is xmin,xmax, ymin,ymax, zmin,zmax — NOT min[3] then max[3].
    let (min, max) = ([f(0), f(8), f(16)], [f(4), f(12), f(20)]);

    let count = posi.payload.len() / 6;
    let mut positions = Vec::with_capacity(count);
    for v in 0..count {
        let q = |i: usize| {
            let o = v * 6 + i * 2;
            u16::from_le_bytes([posi.payload[o], posi.payload[o + 1]]) as f32 / 65535.0
        };
        positions.push([
            min[0] + q(0) * (max[0] - min[0]),
            min[1] + q(1) * (max[1] - min[1]),
            min[2] + q(2) * (max[2] - min[2]),
        ]);
    }

    let raw: Vec<u16> = strp
        .payload
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let triangles = unstitch_strips(&raw)
        .into_iter()
        .filter(|t| t.iter().all(|&i| (i as usize) < count))
        .collect();

    Ok(Some(WaterPatch {
        aabb: (min, max),
        positions,
        triangles,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_chunk;

    /// A synthetic `tern -> WATR` (grid 4, cell 16 m): INFO, LAYR and the
    /// WMAP byte pattern BF1 KAS1 actually ships (`0x11` = two wet cells,
    /// `0x10` = a wet cell then a dry one).
    fn synthetic_tern(wmap: &[u8]) -> Vec<u8> {
        let mut info = Vec::new();
        for v in [4u32, 2] {
            info.extend(v.to_le_bytes());
        }
        for v in [16.0f32, 32.0, 32.0] {
            info.extend(v.to_le_bytes());
        }
        info.extend(1u32.to_le_bytes());
        let mut layr = b"w\0".to_vec();
        layr.extend(10u32.to_le_bytes());
        for v in [-3.0f32, 0.0, 0.0, 1.0, 1.0] {
            layr.extend(v.to_le_bytes());
        }
        layr.extend([255u8, 255, 255, 255]);
        let mut watr = build_chunk(b"INFO", &info);
        watr.extend(build_chunk(b"LAYR", &layr));
        watr.extend(build_chunk(b"WMAP", wmap));
        watr.extend(build_chunk(b"CPAT", &[]));
        let mut tern = build_chunk(b"NAME", b"t\0\0\0");
        tern.extend(build_chunk(b"WATR", &watr));
        build_chunk(b"tern", &tern)
    }

    /// The nibble order, pinned THROUGH the parser: the high nibble is the
    /// lower x. Read low-nibble-first, `0x10` would put the wet cell at x = 3
    /// instead of x = 2 and this fails.
    #[test]
    fn wmap_nibbles_are_high_first() {
        let buf = synthetic_tern(&[0x11u8, 0x10, 0x01, 0x00, 0, 0, 0, 0]);
        let tern = crate::chunks(&buf).next().unwrap().unwrap();
        let w = Water::parse(&tern).unwrap().expect("water parsed");
        assert_eq!(w.info.grid, 4);
        assert_eq!(w.layer.height, -3.0);
        assert_eq!(&w.cells[..8], &[1, 1, 1, 0, 0, 1, 0, 0]);
        // Row 0: x = 0, 1, 2 wet, x = 3 dry. Row 1: only x = 1 wet.
        assert!(w.is_wet(0, 0) && w.is_wet(1, 0) && w.is_wet(2, 0));
        assert!(!w.is_wet(3, 0), "low-nibble-first would put water here");
        assert!(w.is_wet(1, 1) && !w.is_wet(0, 1));
        assert_eq!(w.wet_cells(), 4);
    }
}
