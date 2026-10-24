//! SPU2/VAG ADPCM decode (RE: analysis/re-sound-banks.md, verified recipe).
//! 16-byte blocks: byte0 = (filter<<4)|shift, byte1 = flags, bytes 2..15 =
//! 28 nibbles (low nibble first) → 28 s16 PCM frames per block.

const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

pub fn decode(adpcm: &[u8]) -> Vec<i16> {
    let mut out = Vec::with_capacity(adpcm.len() / 16 * 28);
    let (mut h1, mut h2) = (0i32, 0i32);
    for block in adpcm.as_chunks::<16>().0 {
        let shift = (block[0] & 0x0F) as i32;
        let filter = ((block[0] >> 4) as usize).min(4);
        let (f0, f1) = FILTERS[filter];
        for i in 0..28 {
            let byte = block[2 + i / 2];
            let nibble = if i % 2 == 0 { byte & 0x0F } else { byte >> 4 };
            // sign-extend the 4-bit nibble into the top of an i16, then shift
            let s = (((nibble as i32) << 12) as i16 as i32) >> shift;
            let pcm = (s + (h1 * f0 + h2 * f1) / 64).clamp(-32768, 32767);
            h2 = h1;
            h1 = pcm;
            out.push(pcm as i16);
        }
    }
    out
}

/// Minimal mono s16 WAV writer.
pub fn wav_mono_s16(rate: u32, pcm: &[i16]) -> Vec<u8> {
    let data_len = pcm.len() * 2;
    let mut w = Vec::with_capacity(44 + data_len);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in pcm {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// Goertzel power at `freq` Hz — used by tests to confirm tone frequency.
pub fn goertzel(pcm: &[i16], rate: f64, freq: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * freq / rate;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in pcm {
        let s0 = x as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}
