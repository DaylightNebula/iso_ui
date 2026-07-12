use std::time::Duration;

use magician_vgpu::glam::Vec2;
use ordered_float::OrderedFloat;

use crate::SDFShape;

mod outline;

/// A font that creates characters rendered as SDFElement's.
pub struct SDFFont {
    face: ttf_parser::Face<'static>,
    cache: expiringmap::ExpiringMap<(char, OrderedFloat<f32>), SDFCharEntry>
}

/// A string of `SDFCharEntry`s meant to represent a string in SDFs.
pub struct SDFGlyphString {
    pub chars: Vec<SDFCharEntry>,
    pub dimensions: Vec2
}

/// A single SDF character and its metrics.
#[derive(Clone, Debug)]
pub struct SDFCharEntry {
    pub shape: Option<SDFShape>,
    pub dimensions: Vec2,
    pub advance: f32
}

impl SDFFont {
    /// Create a new font from the bytes of a TTF file.
    pub fn new(bytes: &'static [u8]) -> anyhow::Result<Self> {
        Ok(Self {
            face: ttf_parser::Face::parse(bytes, 0)?,
            cache: expiringmap::ExpiringMap::default()
        })
    }

    /// Measure the width and height of a single line of text
    pub fn measure_text(&self, string: &str, font_size: f32) -> Vec2 {
        let font_size_mult = font_size / self.face.units_per_em() as f32;
        let height = self.face.height() as f32 * font_size_mult;
        let width = string.chars()
            .filter_map(|ch| {
                let Some(id) = self.face.glyph_index(ch)
                    else { return None };
                let advance = self.face.glyph_hor_advance(id)
                    .unwrap_or(0) as f32 * font_size_mult;
                Some(advance)
            })
            .sum();
        Vec2::new(width, height)
    }

    /// Render a string to a `SDFGlyphString` for drawing a string
    /// via SDFs.
    pub fn render_glyph_line(
        &mut self, 
        string: &str, 
        font_size: f32
    ) -> anyhow::Result<SDFGlyphString> {
        // calculate metadata constant across characters
        let font_size_mult = font_size / self.face.units_per_em() as f32;
        let height = self.face.height() as f32 * font_size_mult;
        let mut size_tracker = Vec2::new(0.0, height);

        // build entry for character
        let chars = string.chars()
            .filter_map(|ch| {
                let Some(id) = self.face.glyph_index(ch) 
                    else { return None; };

                // check cache and return from there first
                let cache_key: (char, OrderedFloat<f32>) = (ch, font_size.into());
                let cached = self.cache.get(&cache_key);
                if cached.is_some() { return Some(cached.cloned().unwrap()) }

                // calculate character metadata and build outline
                let mut outline = outline::GlyphBuilder::default();
                let hor_advance = self.face
                    .glyph_hor_advance(id)
                    .unwrap_or(0) as f32 * font_size_mult;
                let bbox = self.face
                    .outline_glyph(id, &mut outline);

                // build character entries
                let char_entry =
                    if let Some(bbox) = bbox {
                        outline.set_bbox(bbox);
                        outline.set_units_per_em(self.face.units_per_em() as f32);

                        size_tracker.x += hor_advance;
                        SDFCharEntry { 
                            shape: Some(outline.build(font_size, Vec2::new(hor_advance / -2.0, height / 2.0 + (self.face.descender() as f32 * font_size_mult)))), 
                            dimensions: Vec2::new(hor_advance, height), 
                            advance: hor_advance
                        }
                    } else {
                        SDFCharEntry { 
                            shape: None, 
                            dimensions: Vec2::ZERO, 
                            advance: hor_advance
                        }
                    };

                // cache and return char_entry
                self.cache.insert(cache_key, char_entry.clone(), Duration::from_secs_f32(1.0));
                return Some(char_entry);
            })
            .collect::<Vec<_>>();

        Ok(SDFGlyphString { chars, dimensions: size_tracker })
    }
}
