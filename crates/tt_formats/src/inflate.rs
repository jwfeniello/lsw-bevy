//! Inflate for TT Games' deflate variant (the "DFLT" blocks inside a `.DAT` archive).
//!
//! It is RFC 1951 with two changes: block type 0 is dynamic Huffman and type 2 is stored
//! (swapped), and a stored block is a 16-bit length followed by the bytes with no complement.

use crate::{Error, Result};

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    count: u32,
}

impl<'a> Bits<'a> {
    fn bits(&mut self, need: u32) -> Result<u32> {
        while self.count < need {
            let byte = *self.data.get(self.pos).ok_or(Error::new("inflate: out of input"))?;
            self.pos += 1;
            self.buf |= (byte as u32) << self.count;
            self.count += 8;
        }
        let value = self.buf & ((1u32 << need) - 1);
        self.buf >>= need;
        self.count -= need;
        Ok(value)
    }
}

/// Canonical Huffman table: number of codes per length, then symbols in code order.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Self {
        let mut count = [0u16; 16];
        for &len in lengths {
            count[len as usize] += 1;
        }
        count[0] = 0;
        let mut offsets = [0u16; 16];
        for len in 1..16 {
            offsets[len] = offsets[len - 1] + count[len - 1];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &len) in lengths.iter().enumerate() {
            if len != 0 {
                symbol[offsets[len as usize] as usize] = sym as u16;
                offsets[len as usize] += 1;
            }
        }
        Self { count, symbol }
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.bits(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return Ok(self.symbol[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(Error::new("inflate: bad Huffman code"))
    }
}

const LENGTH_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CODE_LENGTH_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn codes(bits: &mut Bits, out: &mut Vec<u8>, lit: &Huffman, dist: &Huffman) -> Result<()> {
    loop {
        let sym = lit.decode(bits)? as usize;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let sym = sym - 257;
            if sym >= 29 {
                return Err(Error::new("inflate: bad length symbol"));
            }
            let len = LENGTH_BASE[sym] as usize + bits.bits(LENGTH_EXTRA[sym] as u32)? as usize;
            let dsym = dist.decode(bits)? as usize;
            if dsym >= 30 {
                return Err(Error::new("inflate: bad distance symbol"));
            }
            let distance = DIST_BASE[dsym] as usize + bits.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if distance > out.len() {
                return Err(Error::new("inflate: distance before start of output"));
            }
            let start = out.len() - distance;
            for i in 0..len {
                out.push(out[start + i]);
            }
        }
    }
}

/// Inflate one TT deflate stream into `out` (appending).
pub fn inflate(data: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let mut bits = Bits { data, pos: 0, buf: 0, count: 0 };
    loop {
        let last = bits.bits(1)?;
        match bits.bits(2)? {
            // Stored (RFC type 0, TT type 2): 16-bit length, no complement word.
            2 => {
                bits.buf = 0;
                bits.count = 0;
                let p = bits.pos;
                let header = data.get(p..p + 2).ok_or(Error::new("inflate: truncated stored block"))?;
                let len = u16::from_le_bytes([header[0], header[1]]) as usize;
                let body = data.get(p + 2..p + 2 + len).ok_or(Error::new("inflate: truncated stored block"))?;
                out.extend_from_slice(body);
                bits.pos = p + 2 + len;
            }
            1 => {
                let mut lengths = [0u8; 288];
                for (i, len) in lengths.iter_mut().enumerate() {
                    *len = match i {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    };
                }
                codes(&mut bits, out, &Huffman::new(&lengths), &Huffman::new(&[5u8; 30]))?;
            }
            // Dynamic Huffman (RFC type 2, TT type 0).
            0 => {
                let nlen = bits.bits(5)? as usize + 257;
                let ndist = bits.bits(5)? as usize + 1;
                let ncode = bits.bits(4)? as usize + 4;
                let mut lengths = [0u8; 320];
                for &slot in CODE_LENGTH_ORDER.iter().take(ncode) {
                    lengths[slot] = bits.bits(3)? as u8;
                }
                let code_lengths = Huffman::new(&lengths[..19]);
                lengths = [0u8; 320];
                let mut index = 0;
                while index < nlen + ndist {
                    let sym = code_lengths.decode(&mut bits)?;
                    if sym < 16 {
                        lengths[index] = sym as u8;
                        index += 1;
                        continue;
                    }
                    let (value, repeat) = match sym {
                        16 => {
                            if index == 0 {
                                return Err(Error::new("inflate: repeat with no previous length"));
                            }
                            (lengths[index - 1], 3 + bits.bits(2)? as usize)
                        }
                        17 => (0, 3 + bits.bits(3)? as usize),
                        _ => (0, 11 + bits.bits(7)? as usize),
                    };
                    if index + repeat > nlen + ndist {
                        return Err(Error::new("inflate: too many code lengths"));
                    }
                    lengths[index..index + repeat].fill(value);
                    index += repeat;
                }
                let lit = Huffman::new(&lengths[..nlen]);
                let dist = Huffman::new(&lengths[nlen..nlen + ndist]);
                codes(&mut bits, out, &lit, &dist)?;
            }
            _ => return Err(Error::new("inflate: reserved block type")),
        }
        if last != 0 {
            return Ok(());
        }
    }
}
