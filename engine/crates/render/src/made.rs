//! What the renderer made on the GPU (card HITCH): every buffer, texture,
//! bind group, sampler, shader module and pipeline the render crate creates
//! goes through one of these wrappers, which notes its kind, label and bytes.
//! The windowed game's frame recorder (`game::frame_profile`) takes the list
//! once a frame, so a frame that made GPU objects says which. A process-wide
//! list (the creation sites have no frame in hand); nothing is noted until
//! [`enable`], and a list nobody takes stops at [`CAP`] entries.
//!
//! Wall-clock bookkeeping only: nothing here reaches the picture or the sim.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// One GPU object made.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    /// `buffer`, `texture`, `bind group`, `sampler`, `shader`, `pipeline`.
    pub kind: &'static str,
    /// The descriptor's label ("" when it has none).
    pub label: String,
    /// Bytes of GPU memory (a buffer's size, a texture's texels over every
    /// mip level; 0 for the others) - or of the file, for a caller's note.
    pub bytes: u64,
    /// Wall-clock ms the making took (the create call; a caller's note
    /// times its whole load).
    pub ms: f32,
}

/// Entries kept between two [`take`]s; past it only [`dropped`] counts.
pub const CAP: usize = 4096;

static ON: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static MADE: Mutex<Vec<Made>> = Mutex::new(Vec::new());

/// Start (or stop) noting.
pub fn enable(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Note one object (called by the wrappers below; the game notes its own
/// loads in a frame the same way: a texture file, a sound decoded).
pub fn note(kind: &'static str, label: Option<&str>, bytes: u64, ms: f32) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let mut m = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if m.len() >= CAP {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    m.push(Made {
        kind,
        label: label.unwrap_or("").to_owned(),
        bytes,
        ms,
    });
}

/// Whether [`note`] keeps anything (a caller can skip its own timing).
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// The ms since `t0`.
fn since(t0: std::time::Instant) -> f32 {
    t0.elapsed().as_secs_f32() * 1000.0
}

/// Everything noted since the last call.
pub fn take() -> Vec<Made> {
    std::mem::take(&mut *MADE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Objects not kept because the list was full, since the process started.
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// A texture's bytes over its mip levels (block-compressed formats by their
/// block size; a format without one counts 4 bytes a texel).
pub fn texture_bytes(d: &wgpu::TextureDescriptor<'_>) -> u64 {
    let f = d.format;
    let (bw, bh) = f.block_dimensions();
    let block = u64::from(f.block_copy_size(None).unwrap_or(4));
    let layers = u64::from(d.size.depth_or_array_layers.max(1));
    (0..d.mip_level_count.max(1))
        .map(|m| {
            let w = (d.size.width >> m).max(1).div_ceil(bw);
            let h = (d.size.height >> m).max(1).div_ceil(bh);
            u64::from(w) * u64::from(h) * block * layers
        })
        .sum::<u64>()
        * u64::from(d.sample_count.max(1))
}

pub fn buffer(device: &wgpu::Device, d: &wgpu::BufferDescriptor<'_>) -> wgpu::Buffer {
    let t0 = std::time::Instant::now();
    let made = device.create_buffer(d);
    note("buffer", d.label, d.size, since(t0));
    made
}

pub fn buffer_init(
    device: &wgpu::Device,
    d: &wgpu::util::BufferInitDescriptor<'_>,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    let t0 = std::time::Instant::now();
    let made = device.create_buffer_init(d);
    note("buffer", d.label, d.contents.len() as u64, since(t0));
    made
}

pub fn texture(device: &wgpu::Device, d: &wgpu::TextureDescriptor<'_>) -> wgpu::Texture {
    let t0 = std::time::Instant::now();
    let made = device.create_texture(d);
    note("texture", d.label, texture_bytes(d), since(t0));
    made
}

pub fn bind_group(device: &wgpu::Device, d: &wgpu::BindGroupDescriptor<'_>) -> wgpu::BindGroup {
    let t0 = std::time::Instant::now();
    let made = device.create_bind_group(d);
    note("bind group", d.label, 0, since(t0));
    made
}

pub fn sampler(device: &wgpu::Device, d: &wgpu::SamplerDescriptor<'_>) -> wgpu::Sampler {
    let t0 = std::time::Instant::now();
    let made = device.create_sampler(d);
    note("sampler", d.label, 0, since(t0));
    made
}

pub fn shader_module(
    device: &wgpu::Device,
    d: wgpu::ShaderModuleDescriptor<'_>,
) -> wgpu::ShaderModule {
    let label = d.label;
    let t0 = std::time::Instant::now();
    let made = device.create_shader_module(d);
    note("shader", label, 0, since(t0));
    made
}

pub fn render_pipeline(
    device: &wgpu::Device,
    d: &wgpu::RenderPipelineDescriptor<'_>,
) -> wgpu::RenderPipeline {
    let t0 = std::time::Instant::now();
    let made = device.create_render_pipeline(d);
    note("pipeline", d.label, 0, since(t0));
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_counts_its_mips_and_blocks() {
        let d = |format, mips| wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 256,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        assert_eq!(
            texture_bytes(&d(wgpu::TextureFormat::Rgba8Unorm, 1)),
            256 * 128 * 4
        );
        // 256x128 + 128x64 + 64x32 texels of 4 bytes.
        assert_eq!(
            texture_bytes(&d(wgpu::TextureFormat::Rgba8Unorm, 3)),
            (256 * 128 + 128 * 64 + 64 * 32) * 4
        );
        // BC1: 8 bytes a 4x4 block.
        assert_eq!(
            texture_bytes(&d(wgpu::TextureFormat::Bc1RgbaUnorm, 1)),
            64 * 32 * 8
        );
    }
}
