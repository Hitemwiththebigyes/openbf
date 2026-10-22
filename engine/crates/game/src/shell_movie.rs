//! The BF2 shell's background movie (card BSHLA, `analysis/re-bf2-shell.md`
//! §4): the menu loops the bake transcoded to frame packs
//! (`manifest::shell`), played by the movie property a screen names and
//! decoded on a worker thread so the window never waits on a JPEG.
//!
//! A property plays its segments in the `mcfg`'s order and loops; a screen
//! that names the property already playing keeps it going
//! (`ifelem_shellscreen_fnStartMovie` plays only when it is not already
//! playing, §4 [V]). `[H]` PROVISIONAL: the order - the property's field
//! 0x4d5af670 may pick a segment at random (§4); in order is what the
//! frames cannot tell apart.

use manifest::shell::{FramePack, ShellIndex};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};

/// The shell bake's directory under the assets.
pub fn shell_dir(assets_dir: &Path) -> PathBuf {
    assets_dir.join("common_bf2").join("shell")
}

/// The bake's index; `None` before the card's bake.
pub fn load_index(assets_dir: &Path) -> Option<ShellIndex> {
    let text = pak::read_to_string(shell_dir(assets_dir).join("shell.ron")).ok()?;
    ron::from_str(&text).ok()
}

/// Decode one frame to RGBA8.
pub fn decode(jpeg: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(jpeg), opts);
    let rgba = d.decode().ok()?;
    let info = d.info()?;
    Some((rgba, u32::from(info.width), u32::from(info.height)))
}

type Job = (Arc<FramePack>, usize, u64);
type Decoded = (u64, Vec<u8>, u32, u32);

/// One movie property's packs, loaded.
struct Property {
    name: String,
    packs: Vec<Arc<FramePack>>,
}

pub struct ShellMovie {
    properties: Vec<Property>,
    playing: Option<usize>,
    segment: usize,
    /// Seconds into the playing segment.
    t: f32,
    /// The frame last asked of the worker, by (property, segment, frame).
    asked: Option<(usize, usize, usize)>,
    serial: u64,
    job: Arc<(Mutex<Option<Job>>, Condvar)>,
    done: mpsc::Receiver<Decoded>,
    /// Whether any frame has been shown since the property started.
    pub shown: bool,
}

impl ShellMovie {
    /// Load every menu loop of the bake under `assets_dir`; `None` when the
    /// shell is not baked (the shell then draws over black).
    pub fn load(assets_dir: &Path) -> Option<ShellMovie> {
        let index = load_index(assets_dir)?;
        let dir = shell_dir(assets_dir).join("movies");
        let mut properties = Vec::new();
        for m in &index.movies {
            let packs: Vec<Arc<FramePack>> = m
                .segments
                .iter()
                .filter_map(|f| pak::read(dir.join(f)).ok())
                .filter_map(FramePack::parse)
                .filter(|p| !p.is_empty())
                .map(Arc::new)
                .collect();
            if !packs.is_empty() {
                properties.push(Property {
                    name: m.name.clone(),
                    packs,
                });
            }
        }
        let job: Arc<(Mutex<Option<Job>>, Condvar)> = Arc::new((Mutex::new(None), Condvar::new()));
        let (tx, done) = mpsc::channel();
        let worker = Arc::clone(&job);
        std::thread::Builder::new()
            .name("shell movie".into())
            .spawn(move || loop {
                let (lock, wake) = &*worker;
                let mut slot = lock.lock().unwrap_or_else(|e| e.into_inner());
                while slot.is_none() {
                    slot = wake.wait(slot).unwrap_or_else(|e| e.into_inner());
                }
                let (pack, k, serial) = slot.take().expect("a job");
                drop(slot);
                if let Some((rgba, w, h)) = pack.frame(k).and_then(decode) {
                    if tx.send((serial, rgba, w, h)).is_err() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(ShellMovie {
            properties,
            playing: None,
            segment: 0,
            t: 0.0,
            asked: None,
            serial: 0,
            job,
            done,
            shown: false,
        })
    }

    /// Play `name` (from its start unless it is the one playing).
    pub fn play(&mut self, name: &str) {
        let k = self.properties.iter().position(|p| p.name == name);
        if k != self.playing {
            self.playing = k;
            self.segment = 0;
            self.t = 0.0;
            self.asked = None;
            self.shown = false;
        }
    }

    /// Which (segment, frame) the playing property shows `dt` seconds on.
    fn advance(&mut self, dt: f32) -> Option<(usize, usize)> {
        let p = &self.properties[self.playing?];
        self.t += dt;
        loop {
            let pack = &p.packs[self.segment];
            if self.t < pack.seconds() {
                let k = (self.t * pack.fps.0 as f32 / pack.fps.1 as f32) as usize;
                return Some((self.segment, k.min(pack.len() - 1)));
            }
            self.t -= pack.seconds();
            self.segment = (self.segment + 1) % p.packs.len();
        }
    }

    /// Step the clock by `dt`; the newest decoded frame when one came back
    /// since the last call.
    pub fn frame(&mut self, dt: f32) -> Option<(Vec<u8>, u32, u32)> {
        if let (Some(p), Some((seg, k))) = (self.playing, self.advance(dt)) {
            if self.asked != Some((p, seg, k)) {
                self.asked = Some((p, seg, k));
                self.serial += 1;
                let pack = Arc::clone(&self.properties[p].packs[seg]);
                let (lock, wake) = &*self.job;
                *lock.lock().unwrap_or_else(|e| e.into_inner()) = Some((pack, k, self.serial));
                wake.notify_one();
            }
        }
        let mut newest = None;
        while let Ok((_, rgba, w, h)) = self.done.try_recv() {
            newest = Some((rgba, w, h));
        }
        if newest.is_some() {
            self.shown = true;
        }
        newest
    }

    /// Decode frame `k` of the playing property's segment `seg` now, on
    /// this thread (the goldens and tests).
    pub fn frame_now(&self, name: &str, seg: usize, k: usize) -> Option<(Vec<u8>, u32, u32)> {
        let p = self.properties.iter().find(|p| p.name == name)?;
        p.packs.get(seg)?.frame(k).and_then(decode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Acceptance 3 of card BSHLA: the menu loops decode - segment 0's
    /// frames 0 and 300 are 512 x 448 pictures (pinned by checksum, the
    /// first one of the letterboxed movie's black band) - and the playback
    /// steps through `shell_main`'s two segments. Skips, saying so, on a
    /// bake from before the card.
    #[test]
    fn the_menu_loops_decode_and_play_in_order() {
        let assets = crate::assets::baked_root();
        let Some(mut m) = ShellMovie::load(&assets) else {
            eprintln!(
                "skip: no shell bake under {} (bake common)",
                assets.display()
            );
            return;
        };
        let sum = |rgba: &[u8]| {
            rgba.iter().fold(0x811c_9dc5u32, |h, b| {
                (h ^ u32::from(*b)).wrapping_mul(0x0100_0193)
            })
        };
        let mut sums = Vec::new();
        for k in [0, 300] {
            let (rgba, w, h) = m.frame_now("shell_main", 0, k).expect("frame decodes");
            assert_eq!((w, h), (512, 448));
            // The movie is letterboxed: its top rows are black; 10 s in,
            // its middle is not (frame 0 is the loop's fade from black).
            let row = |y: usize| &rgba[y * 512 * 4..(y + 1) * 512 * 4];
            assert!(row(4)
                .chunks(4)
                .all(|p| p[0] < 24 && p[1] < 24 && p[2] < 24));
            let lit = row(224)
                .chunks(4)
                .any(|p| p[0] > 40 || p[1] > 40 || p[2] > 40);
            assert_eq!(lit, k == 300, "frame {k}");
            sums.push(sum(&rgba));
        }
        assert_ne!(sums[0], sums[1], "frames 0 and 300 differ");
        eprintln!(
            "segment 0 frames 0 / 300 checksums: {:08x} / {:08x}",
            sums[0], sums[1]
        );
        m.play("shell_main");
        let first = m.properties[m.playing.unwrap()].packs[0].seconds();
        assert_eq!(m.advance(0.0), Some((0, 0)));
        assert_eq!(m.advance(first + 0.05).map(|(s, _)| s), Some(1));
        // Back to segment 0 after the second.
        let second = m.properties[m.playing.unwrap()].packs[1].seconds();
        assert_eq!(m.advance(second).map(|(s, _)| s), Some(0));
        // The worker hands a frame back.
        m.play("shell_sub_left");
        let mut got = None;
        for _ in 0..200 {
            got = got.or(m.frame(0.0));
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(got.is_some_and(|(_, w, h)| (w, h) == (512, 448)));
    }
}
