//! Readers for the data formats of LEGO Star Wars: The Complete Saga (Android release).
//!
//! Everything here reads the player's own copy of the game at run time. Layouts come from the
//! notes in `lego-star-wars-formats.md` and are checked against the real files by `ttdump`.

pub mod an3;
pub mod atc;
pub mod charcfg;
pub mod dat;
pub mod fnt;
pub mod ghg;
pub mod inflate;
pub mod rtl;
pub mod ter;

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Little-endian cursor over a byte slice. Every read is bounds-checked.
#[derive(Clone)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).filter(|&e| e <= self.data.len());
        let end = end.ok_or_else(|| Error::new(format!("read of {len} bytes at {:#x} runs past the end", self.pos)))?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn mat4(&mut self) -> Result<[f32; 16]> {
        let mut m = [0.0; 16];
        for value in &mut m {
            *value = self.f32()?;
        }
        Ok(m)
    }
}
