//! Dump mip 0 of PC (`_LVL_PC`) `tex_` chunks to PNG, for eyeballing the
//! [`mungefs::texture_pc`] decoder.
//!
//! ```text
//! pc_tex_dump <lvl-file|-> <out-dir> [label] [texture-name ...]
//! ```
//!
//! `-` reads the LVL from stdin, which is how the Harrisonfog mod LVLs are
//! inspected without unpacking the archives:
//!
//! ```text
//! unzip -p BS1_extract_in_addon_folder.2.zip BS1/data/_LVL_PC/SIDE/BES.lvl \
//!   | cargo run -p mungefs --example pc_tex_dump -- - /tmp/w1 bes
//! ```
//!
//! With no name filter it writes one texture per distinct pixel format it
//! meets, as `<label>_<texture>_<format>.png`. PNGs are written with a
//! stored-deflate encoder inlined below so `mungefs` keeps its single
//! dependency.

use std::collections::BTreeSet;
use std::io::Read;

use mungefs::texture_pc::{find_textures, Texture};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run(&args);
}

fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("usage: pc_tex_dump <lvl-file|-> <out-dir> [label] [texture-name ...]");
        std::process::exit(2);
    }
    let (src, out_dir) = (&args[0], std::path::Path::new(&args[1]));
    let label = args.get(2).cloned().unwrap_or_else(|| {
        std::path::Path::new(src)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "lvl".into())
    });
    let wanted: BTreeSet<String> = args.iter().skip(3).map(|s| s.to_lowercase()).collect();

    let buf = if src == "-" {
        let mut v = Vec::new();
        std::io::stdin().read_to_end(&mut v).expect("read stdin");
        v
    } else {
        std::fs::read(src).unwrap_or_else(|e| panic!("{src}: {e}"))
    };
    let inner = mungefs::open(&buf).expect("ucfb");
    std::fs::create_dir_all(out_dir).expect("create out dir");

    dump_textures(find_textures(inner), out_dir, &label, &wanted);
}

fn dump_textures(
    textures: Vec<Texture>,
    out_dir: &std::path::Path,
    label: &str,
    wanted: &BTreeSet<String>,
) -> BTreeSet<&'static str> {
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    for tex in textures {
        if !wanted.is_empty() && !wanted.contains(&tex.name.to_lowercase()) {
            continue;
        }
        if !tex.unsupported.is_empty() {
            eprintln!("{}: undecodable formats {:?}", tex.name, tex.unsupported);
        }
        for surface in &tex.surfaces {
            let fmt = surface.format;
            // Default mode: one sample per format, and skip the giants.
            if wanted.is_empty() && (seen.contains(fmt.name()) || surface.width > 1024) {
                continue;
            }
            let Some(mip) = surface.mip0() else { continue };
            let path = out_dir.join(format!("{label}_{}_{}.png", tex.name, fmt.name()));
            std::fs::write(&path, encode_png(&mip.rgba, mip.width, mip.height))
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            seen.insert(fmt.name());
            println!(
                "{:<40} {:<9} {:>4}x{:<4} mips={:<2} faces={} -> {}",
                tex.name,
                fmt.name(),
                surface.width,
                surface.height,
                surface.mip_count,
                surface.faces.len(),
                path.display()
            );
        }
    }
    if wanted.is_empty() {
        println!("formats dumped: {:?}", seen);
    }
    seen
}

/// Minimal RGBA8 PNG writer: one uncompressed-deflate zlib stream, so it
/// needs no compression library. Output is large but valid.
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0); // filter type 0 (None)
        raw.extend_from_slice(row);
    }

    let mut z = vec![0x78, 0x01]; // zlib header, deflate/32K window
    for (i, block) in raw.chunks(0xFFFF).enumerate() {
        let last = (i + 1) * 0xFFFF >= raw.len();
        z.push(u8::from(last));
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend(adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(width.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]); // 8-bit, truecolour+alpha, no interlace

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &z);
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(out: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    out.extend((payload.len() as u32).to_be_bytes());
    out.extend(tag);
    out.extend(payload);
    let mut crc = crc32(0xFFFF_FFFF, tag);
    crc = crc32(crc, payload);
    out.extend((!crc).to_be_bytes());
}

fn crc32(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    b << 16 | a
}

#[cfg(test)]
mod tests {
    use super::*;
    use mungefs::texture_pc::{Face, Mip, PixelFormat, Surface};

    #[test]
    fn optional_label_and_sample_selection() {
        let dir = std::env::temp_dir().join(format!("pc_tex_dump_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("Empty.LVL");
        std::fs::write(&input, b"ucfb\0\0\0\0").unwrap();
        let out = dir.join("out");
        run(&[input.display().to_string(), out.display().to_string()]);
        assert!(out.is_dir());
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);

        let texture = |name: &str, width: u32, has_mip: bool| Texture {
            name: name.into(),
            declared: vec![21],
            unsupported: vec![],
            surfaces: vec![Surface {
                format: PixelFormat::A8R8G8B8,
                width,
                height: 1,
                depth: 1,
                mip_count: u32::from(has_mip),
                type_flags: 0,
                faces: vec![Face {
                    mips: if has_mip {
                        vec![Mip {
                            width,
                            height: 1,
                            rgba: vec![255; width as usize * 4],
                        }]
                    } else {
                        vec![]
                    },
                }],
            }],
        };
        for skipped in [texture("giant", 2048, true), texture("missing", 1, false)] {
            let seen = dump_textures(
                vec![
                    skipped,
                    texture("eligible", 1, true),
                    texture("later", 1, true),
                ],
                &out,
                "sample",
                &BTreeSet::new(),
            );
            assert_eq!(seen.into_iter().collect::<Vec<_>>(), ["A8R8G8B8"]);
            assert!(out.join("sample_eligible_A8R8G8B8.png").is_file());
            assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
            std::fs::remove_file(out.join("sample_eligible_A8R8G8B8.png")).unwrap();
        }
        let wanted = BTreeSet::from(["giant".into(), "second".into()]);
        dump_textures(
            vec![texture("GIANT", 2048, true), texture("second", 1, true)],
            &out,
            "named",
            &wanted,
        );
        assert!(out.join("named_GIANT_A8R8G8B8.png").is_file());
        assert!(out.join("named_second_A8R8G8B8.png").is_file());
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
