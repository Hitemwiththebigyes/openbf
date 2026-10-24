//! The glTF BIN chunk builder shared by every GLB emitter in the bake.
//!
//! This was `skin_glb.rs`'s private `Bin`: the one writer of the four that had
//! the abstraction. `glb`, `water_glb` and `terrain_glb` each open-coded the
//! same `bytes.extend_from_slice` + `bufferViews.push(json!(…))` +
//! `accessors.push(json!(…))` triple, which is what made the byte offsets a
//! hand-maintained invariant in four places.
//!
//! Byte-for-byte identical output is the contract here, not a nice-to-have:
//! every baked `.glb` under `engine/assets_baked/` is compared against a fresh
//! bake (`scripts/scratch_bake.sh` + `scripts/cmp_bake.sh`) whenever this file
//! changes.

use serde_json::{json, Value};

/// Accumulates the BIN chunk and the `bufferViews` / `accessors` that index it.
/// Every method appends and returns the new element's index, so a caller reads
/// as the order the file is written in.
#[derive(Default)]
pub struct Bin {
    pub bytes: Vec<u8>,
    pub views: Vec<Value>,
    pub accessors: Vec<Value>,
}

impl Bin {
    /// Append `data` as a buffer view and return its index.
    ///
    /// The 4-byte alignment pad is required by the glTF spec for accessors
    /// whose component type is smaller than 4 bytes. It never fires for what
    /// the bake writes today — positions/normals are VEC3 f32 (12 B), UVs are
    /// VEC2 f32 (8), indices are u32 (4), colours are VEC4 u8 (4) — which is
    /// why moving the three open-coded writers onto it cannot move a byte.
    pub fn view(&mut self, data: &[u8]) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let start = self.bytes.len();
        self.bytes.extend_from_slice(data);
        self.views
            .push(json!({"buffer": 0, "byteOffset": start, "byteLength": data.len()}));
        self.views.len() - 1
    }

    /// Append `data` as a buffer view plus the accessor over it. `extra` is
    /// merged into the accessor object (`min`/`max`, `normalized`, …).
    pub fn accessor(
        &mut self,
        data: &[u8],
        component_type: u32,
        count: usize,
        ty: &str,
        extra: Value,
    ) -> usize {
        let view = self.view(data);
        let mut acc = json!({"bufferView": view, "componentType": component_type, "count": count, "type": ty});
        if let (Value::Object(a), Value::Object(e)) = (&mut acc, extra) {
            a.extend(e);
        }
        self.accessors.push(acc);
        self.accessors.len() - 1
    }

    /// A float accessor from a FLAT slice: `values.len() / components(ty)`
    /// elements of `ty`.
    pub fn f32s(&mut self, values: &[f32], ty: &str, extra: Value) -> usize {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let comps = match ty {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => unreachable!(),
        };
        self.accessor(&bytes, 5126, values.len() / comps, ty, extra)
    }

    /// A SCALAR u32 index accessor.
    pub fn u32s(&mut self, values: &[u32], extra: Value) -> usize {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.accessor(&bytes, 5125, values.len(), "SCALAR", extra)
    }
}

/// The accessor `min`/`max` pair for a position attribute. glTF requires them
/// on POSITION, and `game::assets` reads them for the mesh AABB.
pub fn min_max3<'a>(p: impl IntoIterator<Item = &'a [f32; 3]>) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in p {
        for i in 0..3 {
            min[i] = min[i].min(v[i]);
            max[i] = max[i].max(v[i]);
        }
    }
    (min, max)
}

/// `[[f32; 3]]` as the flat slice `f32s` takes.
pub fn flat3(v: &[[f32; 3]]) -> Vec<f32> {
    v.iter().flatten().copied().collect()
}

/// Pack one rigid mesh with its caller-owned primitives and materials.
pub fn rigid_glb(
    bin: Bin,
    mesh: Value,
    name: &str,
    materials: Option<Value>,
) -> anyhow::Result<Vec<u8>> {
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "openbf bake"},
        "buffers": [{"byteLength": bin.bytes.len()}],
        "bufferViews": bin.views,
        "accessors": bin.accessors,
        "materials": materials,
        "meshes": [mesh],
        "nodes": [{"mesh": 0, "name": name}],
        "scenes": [{"nodes": [0]}],
        "scene": 0
    });
    if materials.is_none() {
        doc.as_object_mut()
            .unwrap()
            .retain(|key, _| key != "materials");
    }
    Ok(crate::glb::pack_glb(&serde_json::to_vec(&doc)?, &bin.bytes))
}
