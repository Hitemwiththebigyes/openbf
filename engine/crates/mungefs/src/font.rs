//! Bitmap fonts: the `font` chunks of BF2 CORE.LVL (`gamefont_tiny`,
//! `_small`, `_medium`, `_large`; `analysis/re-hud.md` §1.2).
//!
//! ```text
//! font
//!   NAME  the font's name
//!   HEAD  u16 glyph count, u8 page count, u8 line height (px)
//!   FTEX  per page: NAME + a normal tex_ (the glyph sheet)
//!   FBOD  glyph count × 24 B: u16 char code, u8 page, u8 advance,
//!         u8 x0, u8 x1, u8 y0, u8 y1 (the glyph box in texels),
//!         f32 u0, u1, v0, v1
//! ```

use crate::le::{f32_at, u16_at};
use crate::texture::Texture;
use crate::{Chunk, UcfbError};

/// One glyph of an `FBOD` record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub code: u16,
    pub page: u8,
    /// Pen advance in texels.
    pub advance: u8,
    pub x0: u8,
    pub x1: u8,
    pub y0: u8,
    pub y1: u8,
    /// `[u0, u1, v0, v1]` on the page sheet.
    pub uv: [f32; 4],
}

/// A parsed `font` chunk.
pub struct Font {
    pub name: String,
    pub line_height: u8,
    /// The glyph sheets, in page order.
    pub pages: Vec<Texture>,
    pub glyphs: Vec<Glyph>,
}

const GLYPH_BYTES: usize = 24;

/// Parse a `font` chunk.
pub fn parse(chunk: &Chunk<'_>) -> Result<Font, UcfbError> {
    let name = chunk
        .child_string(b"NAME")
        .ok_or(UcfbError::Truncated(chunk.offset))?;
    let head = chunk
        .child(b"HEAD")
        .ok_or(UcfbError::Truncated(chunk.offset))?;
    if head.payload.len() < 4 {
        return Err(UcfbError::Truncated(head.offset));
    }
    let count = u16_at(head.payload, 0) as usize;
    let page_count = head.payload[2] as usize;
    let line_height = head.payload[3];

    let mut pages = Vec::new();
    if let Some(ftex) = chunk.child(b"FTEX") {
        for c in ftex.children() {
            let c = c?;
            if &c.tag == b"tex_" {
                pages.push(Texture::parse(&c)?);
            }
        }
    }
    if pages.len() != page_count {
        return Err(UcfbError::Malformed(format!(
            "font {name}: HEAD says {page_count} pages, FTEX holds {}",
            pages.len()
        )));
    }

    let body = chunk
        .child(b"FBOD")
        .ok_or(UcfbError::Truncated(chunk.offset))?;
    if body.payload.len() < count * GLYPH_BYTES {
        return Err(UcfbError::Truncated(body.offset));
    }
    let glyphs = body
        .payload
        .as_chunks::<GLYPH_BYTES>()
        .0
        .iter()
        .take(count)
        .map(|g| Glyph {
            code: u16_at(g, 0),
            page: g[2],
            advance: g[3],
            x0: g[4],
            x1: g[5],
            y0: g[6],
            y1: g[7],
            uv: [f32_at(g, 8), f32_at(g, 12), f32_at(g, 16), f32_at(g, 20)],
        })
        .collect();
    Ok(Font {
        name,
        line_height,
        pages,
        glyphs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Data-gated: CORE.LVL's four fonts, 226 glyphs each, pages 1 / 3 / 3 /
    /// 2, and the medium `'!'` of `re-hud.md` §1.2.
    #[test]
    fn core_carries_the_four_game_fonts() {
        let p = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/CORE.LVL"
        );
        let Ok(buf) = std::fs::read(p) else {
            eprintln!("SKIP: extracted BF2 CORE.LVL not present");
            return;
        };
        let inner = crate::open(&buf).unwrap();
        let fonts: Vec<Font> = crate::content_chunks(inner)
            .unwrap()
            .iter()
            .filter(|c| &c.tag == b"font")
            .map(|c| parse(c).unwrap())
            .collect();
        let mut got: Vec<(&str, usize, usize, u8)> = fonts
            .iter()
            .map(|f| {
                (
                    f.name.as_str(),
                    f.glyphs.len(),
                    f.pages.len(),
                    f.line_height,
                )
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                ("gamefont_large", 226, 1, 22),
                ("gamefont_medium", 226, 3, 18),
                ("gamefont_small", 226, 3, 18),
                ("gamefont_tiny", 226, 2, 15),
            ]
        );
        let medium = fonts.iter().find(|f| f.name == "gamefont_medium").unwrap();
        let bang = medium
            .glyphs
            .iter()
            .find(|g| g.code == u16::from(b'!'))
            .unwrap();
        assert_eq!(
            (bang.page, bang.advance, bang.x0, bang.x1, bang.y0, bang.y1),
            (0, 5, 0, 4, 0, 16)
        );
        assert!((bang.uv[0] - 0.1328).abs() < 1e-3 && (bang.uv[3] - 0.1328).abs() < 1e-3);
        assert_eq!(medium.pages[0].name, "gamefont_medium_tex0");
    }
}
