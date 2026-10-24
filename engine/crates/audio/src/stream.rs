//! Streamed music (card AMBM): a take of a PS2 stream is decoded block by
//! block from the bake's byte-for-byte ADPCM stream bodies through `pak`
//! while it plays (the maintainer: music stays streamed).
//!
//! A take is SPU2 ADPCM (16-byte blocks of 28 frames, `bake::adpcm`'s
//! recipe). A mono take is one run. A stereo take is laid out in the
//! stream's 16 KiB blocks, a left block then a right block, and the index's
//! `size` counts both channels: each channel holds `size / 2` bytes and the
//! last block pair is padded with silent ADPCM (measured on BF1 NAB.LVL's
//! `nabcw_music`: consecutive 16 KiB blocks pair up at correlation 0.48
//! left-right against -0.14 right-left, and the last pair of a take holds
//! `size / 2 mod 16 KiB` bytes of audio and zeros in each block).
//!
//! [`TakeReader`] is the pure decoder (no mixer); [`MusicVoice`] plays one
//! through kira's streaming sound.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Where a take's ADPCM lives and how to read it.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamTake {
    /// The baked ADPCM stream body.
    pub path: PathBuf,
    /// Byte offset of the take's first block in that file.
    pub offset: u64,
    /// ADPCM bytes of audio, all channels, without the block padding.
    pub size: usize,
    pub channels: u32,
    pub frequency: u32,
    /// The stream's block size (16 KiB on every stream of both discs).
    pub block_size: usize,
}

impl StreamTake {
    /// ADPCM bytes of one channel.
    fn channel_bytes(&self) -> usize {
        self.size / self.channels.max(1) as usize / 16 * 16
    }

    /// Frames per channel.
    pub fn frames(&self) -> usize {
        self.channel_bytes() / 16 * 28
    }

    pub fn secs(&self) -> f32 {
        self.frames() as f32 / self.frequency.max(1) as f32
    }
}

const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// One channel's ADPCM predictor.
#[derive(Debug, Clone, Copy, Default)]
struct History(i32, i32);

impl History {
    fn decode(&mut self, adpcm: &[u8], out: &mut Vec<f32>) {
        for block in adpcm.as_chunks::<16>().0 {
            let shift = i32::from(block[0] & 0x0F);
            let (f0, f1) = FILTERS[usize::from(block[0] >> 4).min(4)];
            for i in 0..28 {
                let byte = block[2 + i / 2];
                let nibble = if i % 2 == 0 { byte & 0x0F } else { byte >> 4 };
                let s = i32::from((i32::from(nibble) << 12) as i16) >> shift;
                let pcm = (s + (self.0 * f0 + self.1 * f1) / 64).clamp(-32768, 32767);
                self.1 = self.0;
                self.0 = pcm;
                out.push(pcm as f32 / 32768.0);
            }
        }
    }
}

/// A take decoded one stream block (per channel) at a time.
pub struct TakeReader {
    take: StreamTake,
    file: pak::Reader,
    /// The next block to decode.
    block: usize,
    history: [History; 2],
}

impl TakeReader {
    pub fn open(take: StreamTake) -> std::io::Result<TakeReader> {
        let file = pak::open(&take.path)?;
        Ok(TakeReader {
            take,
            file,
            block: 0,
            history: [History::default(); 2],
        })
    }

    pub fn take(&self) -> &StreamTake {
        &self.take
    }

    /// Frames a full block holds per channel.
    pub fn block_frames(&self) -> usize {
        self.take.block_size / 16 * 28
    }

    /// Start again at the block that holds `frame`; returns that block's
    /// first frame. The predictor restarts from silence (exact at frame 0).
    pub fn seek(&mut self, frame: usize) -> usize {
        self.block = frame / self.block_frames().max(1);
        self.history = [History::default(); 2];
        self.block * self.block_frames()
    }

    /// The next block as (left, right) frames -1..1; empty at the end.
    pub fn next_block(&mut self) -> std::io::Result<Vec<[f32; 2]>> {
        let bs = self.take.block_size.max(16);
        let ch = self.take.channels.clamp(1, 2) as usize;
        let done = self.block * bs;
        let left = self.take.channel_bytes().saturating_sub(done).min(bs);
        if left == 0 {
            return Ok(Vec::new());
        }
        let mut pcm: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
        let mut buf = vec![0u8; left];
        for (c, out) in pcm.iter_mut().enumerate().take(ch) {
            let at = self.take.offset + ((self.block * ch + c) * bs) as u64;
            self.file.seek(SeekFrom::Start(at))?;
            self.file.read_exact(&mut buf)?;
            self.history[c].decode(&buf, out);
        }
        self.block += 1;
        let (l, r) = (&pcm[0], if ch == 2 { &pcm[1] } else { &pcm[0] });
        Ok(l.iter().zip(r).map(|(a, b)| [*a, *b]).collect())
    }
}

#[cfg(feature = "backend")]
struct TakeDecoder(TakeReader);

#[cfg(feature = "backend")]
impl kira::sound::streaming::Decoder for TakeDecoder {
    type Error = std::io::Error;

    fn sample_rate(&self) -> u32 {
        self.0.take.frequency
    }

    fn num_frames(&self) -> usize {
        self.0.take.frames()
    }

    fn decode(&mut self) -> Result<Vec<kira::Frame>, Self::Error> {
        let block = self.0.next_block()?;
        Ok(block
            .into_iter()
            .map(|[left, right]| kira::Frame { left, right })
            .collect())
    }

    fn seek(&mut self, index: usize) -> Result<usize, Self::Error> {
        Ok(self.0.seek(index))
    }
}

/// A playing take. Stops when dropped.
pub struct MusicVoice {
    #[cfg(feature = "backend")]
    inner: kira::sound::streaming::StreamingSoundHandle<std::io::Error>,
}

impl MusicVoice {
    /// The level (0 = unity) reached over `secs`.
    pub fn set_volume(&mut self, volume_db: f32, secs: f32) {
        #[cfg(feature = "backend")]
        self.inner.set_volume(
            volume_db,
            kira::Tween {
                duration: std::time::Duration::from_secs_f32(secs.max(0.0)),
                ..Default::default()
            },
        );
        #[cfg(not(feature = "backend"))]
        {
            let _ = (volume_db, secs);
        }
    }
}

impl Drop for MusicVoice {
    fn drop(&mut self) {
        #[cfg(feature = "backend")]
        self.inner.stop(kira::Tween::default());
    }
}

impl crate::Audio {
    /// Start a take, 2-D, at `volume_db`; `looping` wraps it at its end.
    pub fn play_stream(
        &mut self,
        take: StreamTake,
        volume_db: f32,
        looping: bool,
    ) -> anyhow::Result<MusicVoice> {
        #[cfg(feature = "backend")]
        {
            use kira::sound::streaming::StreamingSoundData;
            let reader = TakeReader::open(take)?;
            let mut data = StreamingSoundData::from_decoder(TakeDecoder(reader)).volume(volume_db);
            if looping {
                data = data.loop_region(..);
            }
            let inner = self
                .manager
                .play(data)
                .map_err(|e| anyhow::anyhow!("music stream: {e}"))?;
            Ok(MusicVoice { inner })
        }
        #[cfg(not(feature = "backend"))]
        {
            let _ = (take, volume_db, looping);
            Ok(MusicVoice {})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One ADPCM block of filter 0, shift 0 whose 28 nibbles are all `n`.
    fn block(n: u8) -> [u8; 16] {
        let mut b = [n | (n << 4); 16];
        b[0] = 0;
        b[1] = 2;
        b
    }

    /// A stereo take of `pairs` full block pairs and `tail` more bytes per
    /// channel, written as the disc lays one out; left nibble 1, right 2.
    fn stereo_file(name: &str, bs: usize, pairs: usize, tail: usize) -> StreamTake {
        let mut data = vec![0xEEu8; 32];
        let fill = |n: u8, bytes: usize, data: &mut Vec<u8>| {
            for _ in 0..bytes / 16 {
                data.extend_from_slice(&block(n));
            }
            data.resize(data.len() + bs - bytes, 0);
        };
        for _ in 0..pairs {
            fill(1, bs, &mut data);
            fill(2, bs, &mut data);
        }
        if tail > 0 {
            fill(1, tail, &mut data);
            fill(2, tail, &mut data);
        }
        let path =
            std::env::temp_dir().join(format!("openbf-stream-{name}-{}.bin", std::process::id()));
        std::fs::write(&path, data).unwrap();
        StreamTake {
            path,
            offset: 32,
            size: 2 * (pairs * bs + tail),
            channels: 2,
            frequency: 48_000,
            block_size: bs,
        }
    }

    #[test]
    fn a_stereo_take_reads_left_then_right_blocks_and_stops_at_its_size() {
        let take = stereo_file("lr", 64, 2, 32);
        assert_eq!(take.frames(), (2 * 64 + 32) / 16 * 28);
        let mut r = TakeReader::open(take.clone()).unwrap();
        let (mut frames, mut blocks) = (Vec::new(), Vec::new());
        loop {
            let b = r.next_block().unwrap();
            if b.is_empty() {
                break;
            }
            blocks.push(b.len());
            frames.extend(b);
        }
        let _ = std::fs::remove_file(&take.path);
        assert_eq!(blocks, [112, 112, 56]);
        assert_eq!(frames.len(), take.frames());
        // Nibble 1 at shift 0 is 0x1000, nibble 2 0x2000: the channels are
        // not mixed and the padding is never played.
        for f in &frames {
            assert_eq!(*f, [4096.0 / 32768.0, 8192.0 / 32768.0]);
        }
    }

    #[test]
    fn a_seek_restarts_at_the_block_that_holds_the_frame() {
        let take = stereo_file("seek", 64, 3, 0);
        let mut r = TakeReader::open(take.clone()).unwrap();
        assert_eq!(r.seek(200), 112);
        let mut n = 0;
        while let Ok(b) = r.next_block() {
            if b.is_empty() {
                break;
            }
            n += b.len();
        }
        let _ = std::fs::remove_file(&take.path);
        assert_eq!(n, take.frames() - 112);
    }

    #[test]
    fn the_predictor_runs_across_blocks() {
        // Filter 1 (60 / 64 of the last frame), nibble 1: a rising run that
        // must not restart at a block boundary.
        let mut data = Vec::new();
        for _ in 0..8 {
            let mut b = block(1);
            b[0] = 0x10;
            data.extend_from_slice(&b);
        }
        let path = std::env::temp_dir().join(format!("openbf-stream-h-{}.bin", std::process::id()));
        std::fs::write(&path, data).unwrap();
        let take = StreamTake {
            path: path.clone(),
            offset: 0,
            size: 128,
            channels: 1,
            frequency: 22_050,
            block_size: 32,
        };
        let mut r = TakeReader::open(take).unwrap();
        let a = r.next_block().unwrap();
        let b = r.next_block().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((a.len(), b.len()), (56, 56));
        assert!(b[0][0] > a[55][0] - 1e-6, "{} then {}", a[55][0], b[0][0]);
        assert_eq!(a[0], [4096.0 / 32768.0; 2], "mono goes to both sides");
    }

    /// A take starts on the real mixer where there is one, takes a level
    /// and stops when dropped; a no-op without.
    #[test]
    fn a_take_plays_through_the_mixer_and_stops_on_drop() {
        let take = stereo_file("mix", 64, 4, 0);
        let Some(mut audio) = crate::Audio::new() else {
            let _ = std::fs::remove_file(&take.path);
            eprintln!("SKIP: no audio device");
            return;
        };
        let mut v = audio.play_stream(take.clone(), -60.0, true).expect("voice");
        v.set_volume(-50.0, 1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(50));
        drop(v);
        let _ = std::fs::remove_file(&take.path);
        let mut gone = take;
        gone.path.set_extension("absent");
        if crate::Audio::backend_compiled() {
            assert!(audio.play_stream(gone, 0.0, false).is_err());
        }
    }
}
