//! The items on the field drawn (card PWUP, `analysis/re-powerups.md` §4):
//! every `Sim::pickups` entry as its class's model (`GeometryName`, baked in
//! the common layer: `common_<game>/models/`), upright at the item, facing
//! the heading it was dropped with. BF1 stops drawing an item when 1 s of
//! life is left (hidden for its last 2 s, still there); BF2 draws it to the
//! end. PROVISIONAL: BF2's 1 s fade and the `player_shadow` blob under the
//! item are not drawn.
//!
//! Battle classes can be preloaded before the first frame; a class not
//! preloaded uploads lazily when its item first lies on the field. A model
//! that is not found is reported once and draws nothing.

use super::scene::draw_in;
use super::upload::{upload_submeshes, upload_texture_file};
use crate::assets::load_glb;
use anyhow::Result;
use glam::{Mat4, Quat};
use render::{DrawCmd, DrawLayer, LightPath, MeshHandle, Renderer, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// BF1 draws an item only while its life is over this (s; B1 `FUN_001f3cb0`).
pub const BF1_HIDE_LIFE: f32 = 1.0;

/// One drawn submesh of an item model.
struct Part {
    mesh: MeshHandle,
    texture: Option<TextureHandle>,
    layer: DrawLayer,
    light: LightPath,
}

/// The item models uploaded so far, by geometry name.
#[derive(Default)]
pub struct PickupVisuals {
    /// `common_<game>`, the map's game's; `None` draws nothing.
    dir: Option<PathBuf>,
    bf1: bool,
    models: HashMap<String, Vec<Part>>,
    textures: HashMap<String, Option<TextureHandle>>,
    cull: Option<f32>,
}

/// Is an item with `life` seconds left drawn (§4)?
pub fn item_drawn(bf1: bool, life: f32) -> bool {
    !bf1 || life > BF1_HIDE_LIFE
}

impl PickupVisuals {
    pub fn new(assets_dir: &Path, game: &str, cull: Option<f32>) -> Self {
        Self {
            dir: Some(assets_dir.join(format!("common_{game}"))),
            bf1: game.eq_ignore_ascii_case("bf1"),
            cull,
            ..Self::default()
        }
    }

    fn upload(&mut self, r: &mut Renderer, geometry: &str) -> Result<Vec<Part>> {
        let dir = self.dir.clone().unwrap_or_default();
        let cpu = load_glb(&dir.join("models").join(format!("{geometry}.glb")))?;
        let mut parts = Vec::new();
        for (k, (mesh, mat, layer, _)) in upload_submeshes(r, &cpu)?.into_iter().enumerate() {
            let texture = match mat.map(str::to_ascii_lowercase) {
                Some(name) => match self.textures.get(&name) {
                    Some(&t) => t,
                    None => {
                        let t = upload_texture_file(r, &dir.join("textures"), &name)?;
                        self.textures.insert(name, t);
                        t
                    }
                },
                None => None,
            };
            if texture.is_none() && super::route_untextured(mat) != super::Untextured::LookupFailed
            {
                continue;
            }
            parts.push(Part {
                mesh,
                texture,
                layer,
                light: cpu.light_path(k),
            });
        }
        Ok(parts)
    }

    /// Upload every item class `sim` can drop (card HITCH): at the battle's
    /// load, so the first powerup a death leaves reads no file in a frame.
    pub fn preload(&mut self, r: &mut Renderer, sim: &core::sim::Sim) {
        if self.dir.is_none() {
            return;
        }
        for c in &sim.item_classes {
            if c.geometry.is_empty() || self.models.contains_key(&c.geometry) {
                continue;
            }
            let parts = self.upload(r, &c.geometry).unwrap_or_else(|e| {
                eprintln!("pickups: {}: {e:#}", c.geometry);
                Vec::new()
            });
            self.models.insert(c.geometry.clone(), parts);
        }
    }

    /// The draws of every item of `sim` that is drawn now.
    pub fn draws(&mut self, r: &mut Renderer, sim: &core::sim::Sim) -> Vec<DrawCmd> {
        let mut out = Vec::new();
        if self.dir.is_none() {
            return out;
        }
        for p in &sim.pickups {
            let Some(c) = sim.item_classes.get(usize::from(p.class)) else {
                continue;
            };
            if c.geometry.is_empty() || !item_drawn(self.bf1, p.life) {
                continue;
            }
            if !self.models.contains_key(&c.geometry) {
                let parts = self.upload(r, &c.geometry).unwrap_or_else(|e| {
                    eprintln!("pickups: {}: {e:#}", c.geometry);
                    Vec::new()
                });
                self.models.insert(c.geometry.clone(), parts);
            }
            let m = Mat4::from_rotation_translation(Quat::from_rotation_y(p.yaw), p.pos);
            for part in &self.models[&c.geometry] {
                let base = match part.texture {
                    Some(t) => draw_in(part.layer, part.mesh, t, m),
                    None => DrawCmd::flat(part.mesh, [0.6, 0.6, 0.6, 1.0], m),
                };
                out.push(base.with_cull(self.cull).with_light(part.light));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BF1 hides an item for its last 2 s (life 1 -> -1); BF2 never.
    #[test]
    fn bf1_hides_the_item_for_its_last_two_seconds() {
        assert!(item_drawn(true, 30.0));
        assert!(item_drawn(true, 1.01));
        assert!(!item_drawn(true, 1.0));
        assert!(!item_drawn(true, -0.5));
        assert!(item_drawn(false, 1.0));
        assert!(item_drawn(false, -0.9));
    }
}
