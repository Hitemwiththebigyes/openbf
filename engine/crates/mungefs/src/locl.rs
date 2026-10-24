//! Localised text: the `Locl` chunks of each game's CORE.LVL, one per
//! language (`analysis/a2-bake-formats.md` §Localization,
//! `analysis/re-event-messages.md`).
//!
//! ```text
//! Locl
//!   NAME  the language ("english", "spanish", …)
//!   BODY  packed entries, each:
//!         u32  fnv1a(lower-cased key path)
//!         u16  entry size in bytes, this 6-byte header included
//!         UTF-16LE text, NUL padded to the entry size
//! ```
//!
//! The disc stores no key strings, only their hashes: a caller asks by
//! [`key`] of the path it knows (`"common.sides.all.firstnames.01"`) or by
//! the hash the ELF holds as an immediate (BF2's event lines).

use crate::le::{u16_at, u32_at};
use crate::{Chunk, UcfbError};

/// One language's table.
#[derive(Debug, Clone, PartialEq)]
pub struct Locl {
    pub language: String,
    /// `(key hash, text)` in disc order.
    pub entries: Vec<(u32, String)>,
}

/// The hash a key path is stored under.
pub fn key(path: &str) -> u32 {
    crate::fnv1a_lowercase(path)
}

const HEADER_BYTES: usize = 6;

/// Parse a `Locl` chunk.
pub fn parse(chunk: &Chunk<'_>) -> Result<Locl, UcfbError> {
    let language = chunk
        .child_string(b"NAME")
        .ok_or(UcfbError::Truncated(chunk.offset))?;
    let body = chunk
        .child(b"BODY")
        .ok_or(UcfbError::Truncated(chunk.offset))?;
    let b = body.payload;
    let mut entries = Vec::new();
    let mut p = 0;
    while p + HEADER_BYTES <= b.len() {
        let hash = u32_at(b, p);
        let size = u16_at(b, p + 4) as usize;
        // A zero entry ends the table (the BODY's own padding).
        if size < HEADER_BYTES {
            break;
        }
        if p + size > b.len() {
            return Err(UcfbError::Truncated(body.offset + p));
        }
        let units: Vec<u16> = b[p + HEADER_BYTES..p + size]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|u| *u != 0)
            .collect();
        entries.push((hash, String::from_utf16_lossy(&units)));
        p += size;
    }
    Ok(Locl { language, entries })
}

/// The table of `language` among an LVL's content chunks.
pub fn find(content: &[Chunk<'_>], language: &str) -> Result<Option<Locl>, UcfbError> {
    for c in content.iter().filter(|c| &c.tag == b"Locl") {
        if c.child_str(b"NAME")
            .is_some_and(|n| n.eq_ignore_ascii_case(language))
        {
            return parse(c).map(Some);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_chunk as chunk;

    fn entry(hash: u32, text: &str, pad: usize) -> Vec<u8> {
        let units: Vec<u16> = text
            .encode_utf16()
            .chain(std::iter::repeat_n(0, pad))
            .collect();
        let mut out = hash.to_le_bytes().to_vec();
        out.extend(((HEADER_BYTES + units.len() * 2) as u16).to_le_bytes());
        for u in units {
            out.extend(u.to_le_bytes());
        }
        out
    }

    /// Entries are walked by their own size, the text stops at its NUL and
    /// keeps what is not ASCII.
    #[test]
    fn a_body_parses_entry_by_entry() {
        let mut body = entry(0xacc1_eef7, "%s killed %s", 2);
        body.extend(entry(key("a.b"), "Größe", 1));
        body.extend([0u8; 8]);
        let mut payload = chunk(b"NAME", b"english\0");
        payload.extend(chunk(b"BODY", &body));
        let file = chunk(b"Locl", &payload);
        let c = crate::chunks(&file).next().unwrap().unwrap();
        let l = parse(&c).unwrap();
        assert_eq!(l.language, "english");
        assert_eq!(
            l.entries,
            [
                (0xacc1_eef7, "%s killed %s".to_string()),
                (key("A.B"), "Größe".to_string())
            ]
        );
        assert_eq!(find(&[c], "English").unwrap().unwrap(), l);
    }

    /// Data-gated: both discs' English tables, with the lines
    /// `re-event-messages.md` §1 names under the hashes it names.
    #[test]
    fn both_core_lvls_hold_the_event_lines() {
        // Each table holds a few keys twice (2,053 and 4,717 distinct).
        for (game, entries) in [("bf1", 2057), ("bf2", 4722)] {
            let p = format!(
                "{}/../../../extracted/{game}/lvl/DATA/_LVL_PS2/CORE.LVL",
                env!("CARGO_MANIFEST_DIR")
            );
            let Ok(buf) = std::fs::read(&p) else {
                eprintln!("SKIP: {p} not present");
                continue;
            };
            let content = crate::content_chunks(crate::open(&buf).unwrap()).unwrap();
            let l = find(&content, "english").unwrap().expect("english table");
            assert_eq!(l.entries.len(), entries, "{game}");
            let text = |h: u32| {
                l.entries
                    .iter()
                    .find(|(k, _)| *k == h)
                    .map(|(_, t)| t.as_str())
            };
            if game == "bf2" {
                assert_eq!(text(key("common.sides.cis.firstnames.01")), Some("Unit"));
                assert_eq!(text(0xacc1_eef7), Some("%s killed %s"));
                assert_eq!(text(0x6163_33ea), Some("%s killed himself"));
                assert_eq!(text(0xdd27_9cfc), Some("%s captured the %s"));
                assert_eq!(text(0xae1e_c790), Some("%s lost the %s"));
            } else {
                assert_eq!(text(key("game.infomsg.killed")), Some("%s KILLED %s"));
            }
        }
    }
}
