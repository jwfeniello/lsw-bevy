//! Level lights (`.rtl`): a fixed-size table of 128 light records and 32 fog records, stored as
//! the game's own structures (0x8C bytes per light after a 4-byte version word).

use crate::{Reader, Result};

#[derive(Clone, Debug)]
pub struct Light {
    /// 1 ambient, 2/3/6/8 point, 4 directional, 5 camera-relative directional, 7 "anti-light".
    pub kind: u8,
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub colour: [f32; 3],
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub disabled: bool,
    pub has_specular: bool,
    /// 16-bit angles, used by camera-relative lights.
    pub pitch: i16,
    pub yaw: i16,
    pub include_mask: u16,
    pub exclude_mask: u16,
    pub intensity: f32,
    /// Lights that are links in a chain (not -1) are skipped by the game's own scan.
    pub chain: i8,
}

pub fn parse(file: &[u8]) -> Result<Vec<Light>> {
    let mut lights = Vec::new();
    for i in 0..128 {
        let at = 4 + i * 0x8c;
        if at + 0x8c > file.len() {
            break;
        }
        let mut r = Reader::at(file, at);
        let mut v3 = || -> Result<[f32; 3]> { Ok([r.f32()?, r.f32()?, r.f32()?]) };
        let (position, direction, colour) = (v3()?, v3()?, v3()?);
        let mut r = Reader::at(file, at + 0x3c);
        let (inner_radius, outer_radius) = (r.f32()?, r.f32()?);
        let mut r = Reader::at(file, at + 0x58);
        let (kind, flags) = (r.u8()?, r.u8()?);
        let (pitch, yaw, include_mask, exclude_mask) = (r.i16()?, r.i16()?, r.u16()?, r.u16()?);
        let intensity = Reader::at(file, at + 0x6c).f32()?;
        let chain = Reader::at(file, at + 0x7a).u8()? as i8;
        if kind == 0 {
            break;
        }
        lights.push(Light {
            kind,
            position,
            direction,
            colour,
            inner_radius,
            outer_radius,
            disabled: flags & 1 != 0,
            has_specular: flags & 4 != 0,
            pitch,
            yaw,
            include_mask,
            exclude_mask,
            intensity,
            chain,
        });
    }
    Ok(lights)
}
