//! The silkscreen font: metrics of the signed-distance atlas (`assets/fonts/serigrafia.json`, made
//! by `tools/fonts/atlas.py`) and text laid out into glyph quads, in em units. Any renderer that
//! draws quads with the atlas draws text with it.
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphMetrics {
    /// Atlas rectangle (u0, v0, u1, v1).
    pub uv: [f32; 4],
    /// Quad offset from the pen (x right, y up from the baseline) and size, in em.
    pub off: [f32; 2],
    pub size: [f32; 2],
    pub advance: f32,
}

#[derive(Deserialize)]
struct FontFile {
    width: u32,
    height: u32,
    spread: f32,
    ascent: f32,
    descent: f32,
    glyphs: HashMap<String, [f32; 9]>,
}

#[derive(Clone)]
pub struct Font {
    pub width: u32,
    pub height: u32,
    /// Distance range of the field, in em.
    pub spread: f32,
    pub ascent: f32,
    pub descent: f32,
    glyphs: HashMap<char, GlyphMetrics>,
    fallback: GlyphMetrics,
}

/// One glyph placed: centre and half size in em from the text's anchor, and its atlas rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub uv: [f32; 4],
}

impl Font {
    pub fn parse(json: &str) -> Result<Font, String> {
        let f: FontFile = serde_json::from_str(json).map_err(|e| format!("fuente: {e}"))?;
        let mut glyphs = HashMap::new();
        for (k, g) in f.glyphs {
            if let Some(c) = k.chars().next() {
                glyphs.insert(c, GlyphMetrics { uv: [g[0], g[1], g[2], g[3]], off: [g[4], g[5]], size: [g[6], g[7]], advance: g[8] });
            }
        }
        let fallback = glyphs.get(&'?').copied().unwrap_or_default();
        Ok(Font { width: f.width, height: f.height, spread: f.spread, ascent: f.ascent, descent: f.descent, glyphs, fallback })
    }

    pub fn glyph(&self, c: char) -> &GlyphMetrics {
        self.glyphs.get(&c).or_else(|| self.glyphs.get(&c.to_ascii_uppercase())).unwrap_or(&self.fallback)
    }

    /// Width of `text` (em).
    pub fn width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.glyph(c).advance).sum()
    }

    /// Lay out `text` (one line) with its anchor at `align`: 0 left, 0.5 centre, 1 right of its
    /// width, vertically centred on the cap height. Appends quads (em units).
    pub fn layout(&self, text: &str, align: f32, out: &mut Vec<Placed>) {
        let w = self.width(text);
        let mut x = -w * align;
        let cap = self.ascent * 0.72;
        for c in text.chars() {
            let g = self.glyph(c);
            if c != ' ' {
                let half = [g.size[0] * 0.5, g.size[1] * 0.5];
                out.push(Placed { center: [x + g.off[0] + half[0], g.off[1] + half[1] - cap * 0.5], half, uv: g.uv });
            }
            x += g.advance;
        }
    }
}
