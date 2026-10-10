//! Texture decoder for `.atitc` files: a DDS container holding ATC / ATCA (Adreno) blocks.

use crate::{Error, Reader, Result};

pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    /// RGBA8 for every mip level, largest first, concatenated.
    pub rgba: Vec<u8>,
    pub mip_count: u32,
}

fn colour_palette(block: &[u8]) -> [[u8; 3]; 4] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let expand5 = |v: u16| ((v << 3) | (v >> 2)) as i32;
    let expand6 = |v: u16| ((v << 2) | (v >> 4)) as i32;
    let a = [expand5((c0 >> 10) & 31), expand5((c0 >> 5) & 31), expand5(c0 & 31)];
    let b = [expand5((c1 >> 11) & 31), expand6((c1 >> 5) & 63), expand5(c1 & 31)];
    let clamp = |v: i32| v.clamp(0, 255) as u8;
    let mix = |f: &dyn Fn(i32, i32) -> i32| [0, 1, 2].map(|i| clamp(f(a[i], b[i])));
    if c0 & 0x8000 == 0 {
        [mix(&|x, _| x), mix(&|x, y| (2 * x + y) / 3), mix(&|x, y| (x + 2 * y) / 3), mix(&|_, y| y)]
    } else {
        [[0, 0, 0], mix(&|x, y| x - y / 4), mix(&|x, _| x), mix(&|_, y| y)]
    }
}

fn alpha_palette(block: &[u8]) -> [u8; 8] {
    let (a0, a1) = (block[0] as i32, block[1] as i32);
    let mut out = [a0 as u8, a1 as u8, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for i in 1..7i32 {
            out[i as usize + 1] = (((7 - i) * a0 + i * a1) / 7) as u8;
        }
    } else {
        for i in 1..5i32 {
            out[i as usize + 1] = (((5 - i) * a0 + i * a1) / 5) as u8;
        }
        out[6] = 0;
        out[7] = 255;
    }
    out
}

fn decode_level(data: &[u8], width: usize, height: usize, alpha: bool, out: &mut Vec<u8>) -> Result<usize> {
    let block_size = if alpha { 16 } else { 8 };
    let (bw, bh) = (width.div_ceil(4).max(1), height.div_ceil(4).max(1));
    let needed = bw * bh * block_size;
    if data.len() < needed {
        return Err(Error::new("texture data is shorter than its header says"));
    }
    let start = out.len();
    out.resize(start + width * height * 4, 255);
    for by in 0..bh {
        for bx in 0..bw {
            let block = &data[(by * bw + bx) * block_size..][..block_size];
            let colour = &block[block_size - 8..];
            let palette = colour_palette(colour);
            let indices = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]);
            let alphas = alpha.then(|| alpha_palette(block));
            let alpha_bits = if alpha { u64::from_le_bytes(block[0..8].try_into().unwrap()) >> 16 } else { 0 };
            for py in 0..4 {
                for px in 0..4 {
                    let (x, y) = (bx * 4 + px, by * 4 + py);
                    if x >= width || y >= height {
                        continue;
                    }
                    let i = py * 4 + px;
                    let at = start + (y * width + x) * 4;
                    out[at..at + 3].copy_from_slice(&palette[((indices >> (2 * i)) & 3) as usize]);
                    if let Some(alphas) = alphas {
                        out[at + 3] = alphas[((alpha_bits >> (3 * i)) & 7) as usize];
                    }
                }
            }
        }
    }
    Ok(needed)
}

/// Uncompressed PVR version 3 (`.pvrnc`), used for HUD art that must stay sharp. The header
/// names up to four channels and their bit widths; pixels are little-endian words with the
/// first channel in the top bits, except all-8-bit formats, which are plain bytes in order.
fn decode_pvr(file: &[u8]) -> Result<Texture> {
    let mut r = Reader::at(file, 8);
    let names = r.bytes(4)?;
    let bits = r.bytes(4)?;
    if names.iter().all(|&n| n == 0) || bits.iter().any(|&b| b > 8) {
        return Err(Error::new("compressed PVR textures are not supported"));
    }
    let mut r = Reader::at(file, 24);
    let (height, width) = (r.u32()?, r.u32()?);
    let mips = Reader::at(file, 44).u32()?.max(1);
    let metadata = Reader::at(file, 48).u32()? as usize;
    let bytes_per_pixel = bits.iter().map(|&b| b as usize).sum::<usize>().div_ceil(8);
    if !(1..=4).contains(&bytes_per_pixel) {
        return Err(Error::new("unsupported PVR pixel size"));
    }
    let bytewise = bits.iter().all(|&b| b == 8 || b == 0);
    let mut at = 52 + metadata;
    let (mut w, mut h) = (width as usize, height as usize);
    let mut rgba = Vec::new();
    let mut mip_count = 0;
    let mut has_alpha = false;
    for _ in 0..mips {
        let Some(level) = file.get(at..at + w * h * bytes_per_pixel) else { break };
        for pixel in level.chunks_exact(bytes_per_pixel) {
            let mut out = [0u8, 0, 0, 255];
            let word = pixel.iter().rev().fold(0u32, |acc, &b| (acc << 8) | b as u32);
            let mut shift = (bytes_per_pixel * 8) as u32;
            for (i, (&name, &width_bits)) in names.iter().zip(bits).enumerate() {
                if width_bits == 0 {
                    continue;
                }
                let value = if bytewise {
                    pixel[i] as u32
                } else {
                    shift -= width_bits as u32;
                    let raw = (word >> shift) & ((1 << width_bits) - 1);
                    raw * 255 / ((1 << width_bits) - 1)
                };
                match name {
                    b'r' => out[0] = value as u8,
                    b'g' => out[1] = value as u8,
                    b'b' => out[2] = value as u8,
                    b'a' => {
                        out[3] = value as u8;
                        has_alpha = true;
                    }
                    b'l' | b'i' => out[..3].fill(value as u8),
                    _ => {}
                }
            }
            rgba.extend_from_slice(&out);
        }
        at += w * h * bytes_per_pixel;
        mip_count += 1;
        if w == 1 && h == 1 {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    if mip_count == 0 {
        return Err(Error::new("PVR texture has no pixel data"));
    }
    Ok(Texture { width, height, has_alpha, rgba, mip_count })
}

pub fn decode(file: &[u8]) -> Result<Texture> {
    if file.starts_with(b"PVR\x03") {
        return decode_pvr(file);
    }
    let mut r = Reader::new(file);
    if r.bytes(4)? != b"DDS " {
        return Err(Error::new("texture is not a DDS file"));
    }
    let height = Reader::at(file, 12).u32()?;
    let width = Reader::at(file, 16).u32()?;
    let mips = Reader::at(file, 28).u32()?.max(1);
    let fourcc = Reader::at(file, 84).bytes(4)?;
    let has_alpha = match fourcc {
        b"ATC " => false,
        b"ATCA" | b"ATCI" => true,
        other => return Err(Error::new(format!("unsupported texture format {:?}", String::from_utf8_lossy(other)))),
    };
    let mut rgba = Vec::new();
    let mut at = 128;
    let (mut w, mut h) = (width as usize, height as usize);
    let mut mip_count = 0;
    for _ in 0..mips {
        match decode_level(file.get(at..).unwrap_or(&[]), w, h, has_alpha, &mut rgba) {
            Ok(used) => at += used,
            Err(e) if mip_count == 0 => return Err(e),
            Err(_) => break,
        }
        mip_count += 1;
        if w == 1 && h == 1 {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    Ok(Texture { width, height, has_alpha, rgba, mip_count })
}
