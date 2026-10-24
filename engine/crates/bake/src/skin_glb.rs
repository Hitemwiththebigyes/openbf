//! Skinned GLB writer: skeleton nodes + skin (inverse bind matrices) +
//! JOINTS_0/WEIGHTS_0 primitives + animations. The input is a plain,
//! format-independent description that the PS2 softskin/zaa_ parsers fill in
//! (see analysis/re-softskin.md, re-anim-keys.md). Round-tripped by
//! `game::assets::load_skinned_glb` (tests in crates/game/tests/skinned_glb.rs).

use anyhow::{bail, Result};
use glam::{Mat4, Quat, Vec3};
use serde_json::{json, Value};

use crate::gltf_bin::{flat3, min_max3, Bin};

/// One skeleton bone; transform in the parent's space (bind pose).
#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    /// xyzw
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub struct SkinnedSegment {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Skeleton bone indices (4 per vertex; unused slots 0 with weight 0).
    pub joints: Vec<[u8; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub triangles: Vec<[u32; 3]>,
    /// Material name (source texture), resolved by the runtime manifest.
    pub texture: String,
    /// The segment is composited: its material is written with glTF
    /// `"alphaMode": "BLEND"`. `false` writes no `alphaMode` at all, which
    /// glTF reads as OPAQUE — the sheet's alpha ignored. The caller decides;
    /// for a PS2 side model that is `crate::soldier::to_skinned_model`.
    pub blend: bool,
    /// The segment's specular second pass (`crate::spec::spec_pass`), written
    /// as the material's `extras.spec_pass` exactly as the rigid bake writes
    /// it (`crate::glb::model_to_glb`); `None` writes no extras (queue SPEC2).
    pub spec: Option<crate::spec::SpecPass>,
    /// A BF2 cloth hung on the model (`crate::cloth`), written as the
    /// material's `extras.cloth`: the PS2 draws it as its own object, so
    /// what measures the model's box leaves it out (`game::roster::AIM_SIZE`).
    pub cloth: bool,
}

#[derive(Debug, Clone)]
pub struct SkinnedModel {
    pub name: String,
    pub bones: Vec<Bone>,
    pub segments: Vec<SkinnedSegment>,
}

/// Per-bone keys, one per frame (parent space).
#[derive(Debug, Clone)]
pub struct ClipTrack {
    pub bone: usize,
    /// xyzw per frame
    pub rotations: Vec<[f32; 4]>,
    pub translations: Vec<[f32; 3]>,
}

#[derive(Debug, Clone)]
pub struct Clip {
    pub name: String,
    pub fps: f32,
    pub frames: u32,
    pub tracks: Vec<ClipTrack>,
}

/// Model-space bind transforms from the parent chain.
pub fn global_binds(bones: &[Bone]) -> Vec<Mat4> {
    fn resolve(i: usize, bones: &[Bone], memo: &mut [Option<Mat4>], depth: usize) -> Mat4 {
        if let Some(m) = memo[i] {
            return m;
        }
        assert!(depth <= bones.len(), "bone parent cycle at {i}");
        let local = Mat4::from_rotation_translation(
            Quat::from_array(bones[i].rotation),
            Vec3::from_array(bones[i].translation),
        );
        let m = match bones[i].parent {
            Some(p) => resolve(p, bones, memo, depth + 1) * local,
            None => local,
        };
        memo[i] = Some(m);
        m
    }
    let mut memo = vec![None; bones.len()];
    (0..bones.len())
        .map(|i| resolve(i, bones, &mut memo, 0))
        .collect()
}

/// Serialize a skinned model and its clips to GLB bytes.
pub fn skinned_to_glb(m: &SkinnedModel, clips: &[Clip]) -> Result<Vec<u8>> {
    if m.bones.is_empty() {
        bail!("skinned model {} has no bones", m.name);
    }
    let n_bones = m.bones.len();
    let mut bin = Bin::default();

    // Nodes 0..n_bones = bones (skeleton order); node n_bones = the skinned mesh.
    let mut nodes: Vec<Value> = Vec::new();
    for (i, b) in m.bones.iter().enumerate() {
        let children: Vec<usize> = m
            .bones
            .iter()
            .enumerate()
            .filter(|(_, c)| c.parent == Some(i))
            .map(|(j, _)| j)
            .collect();
        let mut node =
            json!({"name": b.name, "translation": b.translation, "rotation": b.rotation});
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        nodes.push(node);
    }
    let roots: Vec<usize> = (0..n_bones)
        .filter(|&i| m.bones[i].parent.is_none())
        .collect();

    let ibm: Vec<f32> = global_binds(&m.bones)
        .iter()
        .flat_map(|g| g.inverse().to_cols_array())
        .collect();
    let ibm_acc = bin.f32s(&ibm, "MAT4", json!({}));

    let mut materials = Vec::new();
    let mut primitives = Vec::new();
    for seg in &m.segments {
        let n = seg.positions.len();
        if seg.normals.len() != n
            || seg.uvs.len() != n
            || seg.joints.len() != n
            || seg.weights.len() != n
        {
            bail!("segment attribute length mismatch in {}", m.name);
        }
        if let Some(bad) = seg
            .joints
            .iter()
            .flatten()
            .find(|&&j| usize::from(j) >= n_bones)
        {
            bail!("joint {bad} out of range ({n_bones} bones) in {}", m.name);
        }
        let (min, max) = min_max3(&seg.positions);
        let pos = bin.f32s(
            &flat3(&seg.positions),
            "VEC3",
            json!({"min": min, "max": max}),
        );
        let nrm = bin.f32s(&flat3(&seg.normals), "VEC3", json!({}));
        let uv = bin.f32s(
            &seg.uvs.iter().flatten().copied().collect::<Vec<f32>>(),
            "VEC2",
            json!({}),
        );
        let joints_bytes: Vec<u8> = seg.joints.iter().flatten().copied().collect();
        let joints = bin.accessor(&joints_bytes, 5121, n, "VEC4", json!({}));
        let weights = bin.f32s(
            &seg.weights.iter().flatten().copied().collect::<Vec<f32>>(),
            "VEC4",
            json!({}),
        );
        let idx: Vec<u32> = seg.triangles.iter().flatten().copied().collect();
        let indices = bin.u32s(&idx, json!({}));
        let mut material = json!({"name": seg.texture});
        if seg.blend {
            material["alphaMode"] = json!("BLEND");
        }
        if let Some(p) = seg.spec {
            material["extras"] = json!({ "spec_pass": p.to_json() });
        }
        if seg.cloth {
            material["extras"]["cloth"] = json!(true);
        }
        materials.push(material);
        primitives.push(json!({
            "attributes": {"POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv, "JOINTS_0": joints, "WEIGHTS_0": weights},
            "indices": indices,
            "material": materials.len() - 1,
            "mode": 4
        }));
    }
    // No segments: a skeleton and its clips alone (a walker's, card WSKN,
    // whose mesh is its rigid GLB with `JOINTS_0`), with no mesh node.
    let mesh_node = (!m.segments.is_empty()).then(|| {
        nodes.push(json!({"name": m.name, "mesh": 0, "skin": 0}));
        nodes.len() - 1
    });

    let mut animations = Vec::new();
    for clip in clips {
        let frames = clip.frames as usize;
        if frames == 0 {
            bail!("clip {} has no frames", clip.name);
        }
        let times: Vec<f32> = (0..frames).map(|i| i as f32 / clip.fps).collect();
        let input = bin.f32s(
            &times,
            "SCALAR",
            json!({"min": [times[0]], "max": [times[frames - 1]]}),
        );
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for t in &clip.tracks {
            if t.bone >= n_bones {
                bail!("clip {} track bone {} out of range", clip.name, t.bone);
            }
            if t.rotations.len() != frames || t.translations.len() != frames {
                bail!(
                    "clip {} track {} has {} rot / {} trans keys for {frames} frames",
                    clip.name,
                    t.bone,
                    t.rotations.len(),
                    t.translations.len()
                );
            }
            let rot = bin.f32s(
                &t.rotations.iter().flatten().copied().collect::<Vec<f32>>(),
                "VEC4",
                json!({}),
            );
            samplers.push(json!({"input": input, "output": rot, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": t.bone, "path": "rotation"}}));
            let tr = bin.f32s(
                &t.translations
                    .iter()
                    .flatten()
                    .copied()
                    .collect::<Vec<f32>>(),
                "VEC3",
                json!({}),
            );
            samplers.push(json!({"input": input, "output": tr, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": t.bone, "path": "translation"}}));
        }
        animations.push(
            json!({"name": clip.name, "samplers": samplers, "channels": channels,
            "extras": {"fps": clip.fps, "frames": clip.frames}}),
        );
    }

    let mut scene_nodes = roots.clone();
    scene_nodes.extend(mesh_node);
    let meshes = json!([{"name": m.name, "primitives": primitives}]);
    let doc = json!({
        "asset": {"version": "2.0", "generator": "openbf bake (skinned)"},
        "buffers": [{"byteLength": bin.bytes.len()}],
        "bufferViews": bin.views,
        "accessors": bin.accessors,
        "materials": materials,
        "meshes": meshes,
        "skins": [{"name": m.name, "joints": (0..n_bones).collect::<Vec<_>>(), "inverseBindMatrices": ibm_acc, "skeleton": roots[0]}],
        "nodes": nodes,
        "animations": animations,
        "scenes": [{"nodes": scene_nodes}],
        "scene": 0
    });
    let mut doc = doc;
    if mesh_node.is_none() {
        // glTF allows no empty top-level array.
        for k in ["meshes", "materials"] {
            doc.as_object_mut().map(|o| o.remove(k));
        }
    }
    Ok(crate::glb::pack_glb(&serde_json::to_vec(&doc)?, &bin.bytes))
}
