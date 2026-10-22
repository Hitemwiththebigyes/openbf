//! The BF2 HUD on screen (card BHUD2): loads what `bake::hud` wrote, runs
//! `ui::hud_tree` / `ui::hud_draw` over a [`ui::hud::HudModel`] each frame and
//! paints the result through `render::hud::HudPass`. Art is found by name and
//! loaded the first time an element shows it: sheets and font pages as KTX2,
//! meshes as GLB — the HUD's own in `common_bf2`, the weapons' in the side
//! bakes (the PS2's weapon icons are the weapons' own models, `re-hud.md` §1.2).

use anyhow::{Context, Result};
use render::hud::{HudBatch, HudPass, HudTexture, HudVertex};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use ui::hud::HudModel;
use ui::hud_draw::{HudArt, HudMesh, HudMeshPart};
use ui::hud_tree::{Hud, Kind};

/// How long a first frame lets the fades run before it is drawn: a capture
/// shows the settled HUD, not the first instant of its 3 s fade-in.
const SETTLE_SECONDS: f32 = 10.0;

pub struct HudView {
    hud: Hud,
    art: HudArt,
    /// Built at the first paint, where the device is at hand.
    pass: Option<HudPass>,
    format: wgpu::TextureFormat,
    display: Option<wgpu::TextureFormat>,
    /// Where sheets and font pages are looked for, in order.
    texture_dirs: Vec<PathBuf>,
    /// Every drawable mesh by lower-cased name.
    mesh_files: HashMap<String, PathBuf>,
    textures: HashMap<String, Option<HudTexture>>,
    missing: HashSet<String>,
    verts: Vec<HudVertex>,
    indices: Vec<u32>,
    /// The laid-out batches, their sheets still by name.
    batches: Vec<ui::hud_draw::HudBatch>,
    /// Per drawn element of the last layout: its index and window box.
    boxes: Vec<(usize, [f32; 4])>,
    /// The squad cross's parts of the last layout.
    squad: Vec<(ui::hud_squad::Part, [f32; 4], f32)>,
    settled: bool,
    /// The shell movie's texture and the frame waiting to be written into
    /// it (card BSHLA).
    movie: Option<HudTexture>,
    movie_frame: Option<(Vec<u8>, u32, u32)>,
}

fn read_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    ron::from_str(&pak::read_to_string(path).with_context(|| path.display().to_string())?)
        .with_context(|| path.display().to_string())
}

/// `name` → file for every `*.glb` in `dirs`; the first directory wins.
fn glb_index(dirs: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for d in dirs {
        let Ok(files) = pak::read_dir(d) else {
            continue;
        };
        for p in files {
            if p.extension().is_some_and(|x| x == "glb") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.entry(stem.to_ascii_lowercase()).or_insert(p);
                }
            }
        }
    }
    out
}

/// The HUD's own art (`hud_*`) composites by its sheets' alpha
/// (`re-hud-icons.md` §2); any other model the HUD shows is a world model
/// drawn by its material, and an opaque part's sheet alpha is not coverage
/// (`[M]` the PS2 draws the clone rifle and pistol solid, card HUDW's frames;
/// their sheet's alpha plane is the gloss of MTRL flags 0x10089).
fn load_mesh(path: &Path) -> Result<HudMesh> {
    let cpu = crate::assets::load_glb(path)?;
    let art = path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.to_ascii_lowercase().starts_with("hud_"));
    let parts = cpu
        .submeshes
        .iter()
        .map(|(start, count, material, layer, _)| {
            // Re-index the part's own vertices: a weapon model is laid out
            // on the CPU every frame, so carry only what it draws.
            let mut remap: HashMap<u32, u32> = HashMap::new();
            let mut texture = material.to_ascii_lowercase();
            if !art && *layer == render::DrawLayer::Opaque {
                texture.push_str(ui::hud_draw::OPAQUE_SHEET);
            }
            let mut part = HudMeshPart {
                texture,
                ..Default::default()
            };
            for i in &cpu.indices[*start as usize..(*start + *count) as usize] {
                let k = *remap.entry(*i).or_insert_with(|| {
                    part.positions.push(cpu.positions[*i as usize]);
                    part.uvs
                        .push(cpu.uvs.get(*i as usize).copied().unwrap_or([0.0; 2]));
                    part.colors.push(
                        cpu.colors
                            .get(*i as usize)
                            .map_or([1.0; 4], |c| c.map(|b| f32::from(b) / 255.0)),
                    );
                    part.positions.len() as u32 - 1
                });
                part.indices.push(k);
            }
            part
        })
        .collect();
    Ok(HudMesh { parts })
}

impl HudView {
    /// [`HudView::load`] for `r`'s frames.
    pub fn for_renderer(assets_dir: &Path, r: &render::Renderer) -> Result<Self> {
        Self::load(assets_dir, r.surface_format(), r.display_view_format())
    }

    /// Load the 1-player HUD from the bake at `assets_dir`, to be painted
    /// into frames of `format` through a `display` view where there is one
    /// (`render::Renderer::display_view_format`). Touches no GPU: the pass is
    /// built at the first paint. An error means the bake holds no BF2 HUD
    /// (`bake common` writes it).
    pub fn load(
        assets_dir: &Path,
        format: wgpu::TextureFormat,
        display: Option<wgpu::TextureFormat>,
    ) -> Result<Self> {
        let common = assets_dir.join("common_bf2");
        let layout: manifest::hud::HudLayout = read_ron(&common.join("hud/1playerhud.ron"))?;
        let transforms: manifest::hud::HudLayout = read_ron(&common.join("hud/hudtransforms.ron"))?;
        let index: manifest::hud::HudIndex = read_ron(&common.join("hud/index.ron"))?;
        let mut art = HudArt::default();
        for f in &index.fonts {
            let font: manifest::hud::HudFont = read_ron(&common.join(format!("fonts/{f}.ron")))?;
            art.fonts.insert(f.to_ascii_lowercase(), font);
        }
        // BF1's own tiny font for the squad cross's labels (card SQHUD); a
        // bake without it falls back to BF2's.
        let bf1_fonts = assets_dir.join("common_bf1/fonts");
        let tiny = ui::hud_squad::FONT;
        if let Ok(font) = read_ron(&bf1_fonts.join(format!("{tiny}.ron"))) {
            art.fonts.insert(tiny.to_owned(), font);
        }
        let mesh_files = glb_index(&[
            common.join("models"),
            assets_dir.join("side_bf2/weapons"),
            assets_dir.join("side_bf1/weapons"),
        ]);
        let mut hud = Hud::new(&layout, &transforms);
        hud.set_fonts(&art.fonts);
        Ok(Self {
            hud,
            art,
            pass: None,
            format,
            display,
            texture_dirs: vec![
                common.join("textures"),
                common.join("fonts"),
                // The shell's own sheets (`logo`, card BSHLA).
                crate::shell_movie::shell_dir(assets_dir).join("textures"),
                bf1_fonts,
                assets_dir.join("side_bf2/textures"),
                assets_dir.join("common_bf1/textures"),
                assets_dir.join("side_bf1/textures"),
            ],
            mesh_files,
            textures: HashMap::new(),
            missing: HashSet::new(),
            verts: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
            boxes: Vec::new(),
            squad: Vec::new(),
            settled: false,
            movie: None,
            movie_frame: None,
        })
    }

    fn texture_file(&self, name: &str) -> Option<PathBuf> {
        self.texture_dirs
            .iter()
            .map(|d| d.join(format!("{name}.ktx2")))
            .find(|p| pak::is_file(p))
    }

    /// Feed `model`, advance the fades by `dt` seconds and lay the HUD out
    /// for a `size` window. The first call settles the fades.
    pub fn update(&mut self, model: &HudModel, dt: f32, size: [u32; 2]) {
        // `[ours]` per row: a BF1 class keys BF2's table by its own name when
        // its geometry has no entry, and a weapon with no BF2 mesh shows its
        // icon sheet instead.
        let mut model = model.clone();
        for (row, w) in [(1, &mut model.weapon), (2, &mut model.weapon2)] {
            let Some(w) = w else {
                continue;
            };
            let files = &self.mesh_files;
            let has = |n: &str| files.contains_key(n);
            let event = format!("player1.weapon{row}.change");
            let listed = |key: &str| {
                self.hud
                    .name_mesh(&event, key, &has)
                    .filter(|mesh| !mesh.eq_ignore_ascii_case(key))
            };
            if listed(&w.key).is_none() && !w.class.is_empty() && listed(&w.class).is_some() {
                w.key.clone_from(&w.class);
            }
            // A model the layout does not place (a BF1 geometry drawn by the
            // fallback alone: no MeshInfo, or one with a rotation and no
            // Position, which puts it at the group's origin) loses to the
            // weapon's icon.
            let placed = |mesh: &str| {
                self.hud
                    .find(&format!("player1weapon{row}mesh"))
                    .is_some_and(|i| {
                        let infos = &self.hud.elems[i].mesh_info;
                        infos
                            .iter()
                            .any(|k| k.mesh == mesh && k.pos != glam::Vec3::ZERO)
                    })
            };
            let drawable = self
                .hud
                .name_mesh(&event, &w.key, &has)
                .is_some_and(|m| w.icon.is_none() || !m.eq_ignore_ascii_case(&w.key) || placed(&m));
            w.icon = match (&w.icon, drawable) {
                (Some(icon), false) => self.texture_file(icon).map(|p| {
                    if !self.art.sheet_aspect.contains_key(icon) {
                        let aspect = pak::read(&p)
                            .ok()
                            .and_then(|b| crate::assets::load_ktx2(&b).ok())
                            .map_or(2.0, |t| t.width as f32 / t.height.max(1) as f32);
                        self.art.sheet_aspect.insert(icon.clone(), aspect);
                    }
                    icon.clone()
                }),
                _ => None,
            };
        }
        {
            let files = &self.mesh_files;
            ui::hud::feed(&mut self.hud, &model, &|n: &str| files.contains_key(n));
        }
        // The minimap's image is the map's own sheet.
        if let Some(dir) = model.map.as_ref().and_then(|m| m.sheet_dir.as_ref()) {
            if !self.texture_dirs.contains(dir) {
                self.texture_dirs.insert(0, dir.clone());
            }
        }
        let first = !self.settled;
        self.settled = true;
        self.hud.advance(if first { SETTLE_SECONDS } else { dt });

        // Load the meshes the shown elements name.
        let wanted: Vec<String> = self
            .hud
            .elems
            .iter()
            .filter(|e| e.kind == Kind::Model3D && e.fader.state != 0)
            .filter_map(|e| e.mesh.clone())
            .chain(model.map.is_some().then(|| "hud_map_fov".to_owned()))
            .filter(|m| !self.art.meshes.contains_key(m) && !self.missing.contains(m))
            .collect();
        for name in wanted {
            match self.mesh_files.get(&name).map(|p| load_mesh(p)) {
                Some(Ok(mesh)) => {
                    self.art.meshes.insert(name, mesh);
                }
                Some(Err(e)) => {
                    eprintln!("hud: mesh {name:?}: {e:#}");
                    self.missing.insert(name);
                }
                None => {
                    self.missing.insert(name);
                }
            }
        }

        let frame = ui::hud_draw::draw(&self.hud, &self.art, [size[0] as f32, size[1] as f32]);
        self.verts.clear();
        self.verts.extend(frame.verts.iter().map(|v| HudVertex {
            pos: v.pos,
            uv: v.uv,
            color: v.color,
        }));
        self.indices = frame.indices;
        self.batches = frame.batches;
        self.boxes = frame.boxes;
        self.squad = frame.squad;
    }

    /// Lay the shell's page `menus` out for a `size` window (card BSHLA):
    /// the movie's last frame under it when `movie` (see
    /// [`HudView::set_movie_frame`]), the HUD tree not at all.
    pub fn update_shell(&mut self, menus: &ui::shell_ps2::ShellMenus, movie: bool, size: [u32; 2]) {
        let movie = movie && (self.movie.is_some() || self.movie_frame.is_some());
        self.hud.shell = Some((menus.clone(), movie));
        let frame = ui::hud_draw::draw(&self.hud, &self.art, [size[0] as f32, size[1] as f32]);
        self.hud.shell = None;
        self.verts.clear();
        self.verts.extend(frame.verts.iter().map(|v| HudVertex {
            pos: v.pos,
            uv: v.uv,
            color: v.color,
        }));
        self.indices = frame.indices;
        self.batches = frame.batches;
        self.boxes = frame.boxes;
        self.squad = frame.squad;
    }

    /// The shell movie's newest frame (RGBA8 display values), written into
    /// its texture at the next paint.
    pub fn set_movie_frame(&mut self, rgba: Vec<u8>, width: u32, height: u32) {
        self.movie_frame = Some((rgba, width, height));
    }

    /// The window box `[x0, y0, x1, y1]` the element called `name` was drawn
    /// in by the last [`HudView::update`]; `None` when it drew nothing.
    pub fn bounds(&self, name: &str) -> Option<[f32; 4]> {
        let i = self.hud.find(name)?;
        self.boxes
            .iter()
            .filter(|(e, _)| *e == i)
            .map(|(_, b)| *b)
            .reduce(|a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })
    }

    /// The squad cross's part `part` as the last [`HudView::update`] drew
    /// it: its window box and alpha.
    pub fn squad_part(&self, part: ui::hud_squad::Part) -> Option<([f32; 4], f32)> {
        self.squad
            .iter()
            .find(|(p, ..)| *p == part)
            .map(|(_, b, a)| (*b, *a))
    }

    /// Where the minimap draws a world point (x, z) in a `size` window
    /// (`ui::hud_map::window_point`).
    pub fn map_point(&self, world: [f32; 2], clamp: bool, size: [u32; 2]) -> Option<[f32; 2]> {
        ui::hud_map::window_point(&self.hud, world, clamp, [size[0] as f32, size[1] as f32])
    }

    /// Paint the last [`HudView::update`] (call from the overlay hook, before
    /// the egui overlay).
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        let (format, display) = (self.format, self.display);
        let pass = self
            .pass
            .get_or_insert_with(|| HudPass::new(device, queue, format, display));
        if let Some((rgba, w, h)) = self.movie_frame.take() {
            let t = match self.movie {
                Some(t) => Some(t),
                None => pass.add_stream_texture(device, queue, w, h),
            };
            if let Some(t) = t {
                pass.write_stream_texture(queue, t, &rgba);
            }
            self.movie = t;
        }
        let mut batches = Vec::with_capacity(self.batches.len());
        for b in &self.batches {
            if b.texture == ui::shell_ps2::MOVIE_SHEET {
                batches.push(HudBatch {
                    texture: self.movie,
                    additive: b.additive,
                    depth: b.depth,
                    first: b.first,
                    count: b.count,
                });
                continue;
            }
            let texture = match self.textures.get(&b.texture) {
                Some(t) => *t,
                None => {
                    // The PS2's own pixels: the HUD's sheets have no PC tier.
                    let (sheet, opaque) = match b.texture.strip_suffix(ui::hud_draw::OPAQUE_SHEET) {
                        Some(sheet) => (sheet, true),
                        None => (b.texture.as_str(), false),
                    };
                    let t = self
                        .texture_dirs
                        .iter()
                        .map(|d| d.join(format!("{sheet}.ktx2")))
                        .find(|p| pak::is_file(p))
                        .and_then(|p| pak::read(p).ok())
                        .and_then(|bytes| crate::assets::load_ktx2(&bytes).ok())
                        .and_then(|mut t| {
                            if opaque {
                                t.rgba.chunks_exact_mut(4).for_each(|p| p[3] = 255);
                            }
                            pass.add_texture(device, queue, &t.rgba, t.width, t.height)
                        });
                    if t.is_none() {
                        eprintln!("hud: no sheet {:?}; drawn untextured", b.texture);
                    }
                    self.textures.insert(b.texture.clone(), t);
                    t
                }
            };
            batches.push(HudBatch {
                texture,
                additive: b.additive,
                depth: b.depth,
                first: b.first,
                count: b.count,
            });
        }
        pass.paint(
            &self.verts,
            &self.indices,
            &batches,
            device,
            queue,
            encoder,
            view,
            size,
        );
    }
}
