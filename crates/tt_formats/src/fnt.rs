//! The game font (`.fnt`): a small header, a table of glyph boxes, a table mapping characters
//! to glyphs, then one DDS texture holding every glyph.
//!
//! Offsets read from the real file: glyph count at 0x1C, character count at 0x20, line height
//! (f32) at 0x24, glyph boxes (right edge x, top y, width as f32, in texture pixels) from 0x6C, then
//! (character code, glyph index) pairs of u16.

use crate::atc::Texture;
use crate::{Error, Reader, Result};
use std::collections::HashMap;

pub struct Font {
    pub height: f32,
    /// Character -> (right edge x, top y, width) of its box in the texture; every box is
    /// `height` tall.
    pub glyphs: HashMap<char, [f32; 3]>,
    pub texture: Texture,
}

pub fn parse(file: &[u8]) -> Result<Font> {
    let glyph_count = Reader::at(file, 0x1c).u32()? as usize;
    let char_count = Reader::at(file, 0x20).u32()? as usize;
    let height = Reader::at(file, 0x24).f32()?;
    if glyph_count > 4096 || char_count > 4096 {
        return Err(Error::new("font header does not look right"));
    }
    let boxes_at = 0x6c;
    let map_at = boxes_at + glyph_count * 12;
    let mut glyphs = HashMap::new();
    for i in 0..char_count {
        let mut r = Reader::at(file, map_at + i * 4);
        let (code, index) = (r.u16()? as u32, r.u16()? as usize);
        if index >= glyph_count {
            continue;
        }
        let mut b = Reader::at(file, boxes_at + index * 12);
        if let Some(c) = char::from_u32(code) {
            glyphs.insert(c, [b.f32()?, b.f32()?, b.f32()?]);
        }
    }
    let dds = file.windows(4).position(|w| w == b"DDS ").ok_or(Error::new("font has no texture"))?;
    let texture = crate::atc::decode(&file[dds..])?;
    Ok(Font { height, glyphs, texture })
}
