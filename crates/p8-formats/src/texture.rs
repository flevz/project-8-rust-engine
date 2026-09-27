//! `.img` textures (Xbox 360), e.g. the HUD sprites in
//! `DATA/COMPRESSED/ZONES/global.pak.xen`.
//!
//! Layout, as read from the balance meter's four textures:
//!
//! - `+0x1C` (u32, big endian): offset of the Direct3D texture header
//!   (`0x28` in every file seen). LIKELY.
//! - `+0x20` (u32): size of the pixel data; the data is the last that many
//!   bytes of the file (at `0x1000` in every file seen). LIKELY.
//! - The Direct3D header is 7 dwords (common, reference count, fences,
//!   identifier, flushes) followed by the 6-dword GPU texture fetch
//!   constant, whose fields follow the Xenos layout: dword 0 bit 31 tiled,
//!   bits 22..30 pitch / 32; dword 1 bits 0..5 format, 6..7 endian swap;
//!   dword 2 width - 1 (bits 0..12) and height - 1 (bits 13..25); dword 5
//!   bit 11 packed mips. CONFIRMED on these files: the widths, heights and
//!   format decode to images that look right.
//!
//! Only what the balance meter needs is decoded: format `0x14` (DXT4/5)
//! with the 8-in-16 endian swap, base level only.
use std::path::Path;

use crate::{pak, qb_key};

/// A decoded texture, RGBA with 8 bits per channel, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("texture too short at {at:#x}"))
}

/// Xbox 360 tiled surface address (the public `XGAddress2DTiledOffset`
/// formula): byte offset of block (`x`, `y`) in a surface `pitch` blocks
/// wide whose blocks are `1 << log2_bpp` bytes.
pub fn tiled_offset(x: u32, y: u32, pitch: u32, log2_bpp: u32) -> u32 {
    let aligned = (pitch + 31) & !31;
    let macro_ = ((x >> 5) + (y >> 5) * (aligned >> 5)) << (log2_bpp + 7);
    let micro = ((x & 7) + ((y & 0xE) << 2)) << log2_bpp;
    let off = macro_ + ((micro & !0xF) << 1) + (micro & 0xF) + ((y & 1) << 4);
    ((off & !0x1FF) << 3) + ((y & 16) << 7) + ((off & 0x1C0) << 2) + ((((y & 8) >> 2) + (x >> 3)) & 3) * 64 + (off & 0x3F)
}

fn log2_ceil(v: u32) -> u32 {
    32 - v.saturating_sub(1).leading_zeros()
}

/// Where the base level starts inside its tile when the texture's mips are
/// packed and it is 16 texels or less on one side: 16 texels along the
/// longer side's other axis. CONFIRMED for a taller-than-wide texture
/// (`balancearrow_glow`, 16 x 32, at x = 16: exactly its non-empty
/// blocks); LIKELY for the other cases (same public rule).
fn packed_offset(width: u32, height: u32) -> (u32, u32) {
    let (lw, lh) = (log2_ceil(width), log2_ceil(height));
    if lw.min(lh) > 4 {
        (0, 0)
    } else if lw > lh {
        (0, 16)
    } else {
        (16, 0)
    }
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = (c >> 11) & 31;
    let g = (c >> 5) & 63;
    let b = c & 31;
    [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
}

/// Decode one DXT5 block (little-endian after the swap) into 16 pixels.
fn dxt5_block(b: &[u8; 16]) -> [[u8; 4]; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut alpha = [0u8; 8];
    alpha[0] = a0 as u8;
    alpha[1] = a1 as u8;
    if a0 > a1 {
        for i in 0..6 {
            alpha[2 + i] = ((a0 * (6 - i as u32) + a1 * (1 + i as u32)) / 7) as u8;
        }
    } else {
        for i in 0..4 {
            alpha[2 + i] = ((a0 * (4 - i as u32) + a1 * (1 + i as u32)) / 5) as u8;
        }
        alpha[6] = 0;
        alpha[7] = 255;
    }
    let abits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | x as u64);
    let c0 = rgb565(u16::from_le_bytes([b[8], b[9]]));
    let c1 = rgb565(u16::from_le_bytes([b[10], b[11]]));
    let mix = |p: u32, q: u32| -> [u8; 3] {
        let f = |i: usize| ((c0[i] as u32 * p + c1[i] as u32 * q) / 3) as u8;
        [f(0), f(1), f(2)]
    };
    let colours = [c0, c1, mix(2, 1), mix(1, 2)];
    let cbits = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
    let mut out = [[0u8; 4]; 16];
    for (i, px) in out.iter_mut().enumerate() {
        let c = colours[(cbits >> (2 * i) & 3) as usize];
        *px = [c[0], c[1], c[2], alpha[(abits >> (3 * i) & 7) as usize]];
    }
    out
}

/// Decode an `.img` file's base level.
pub fn decode_img(bytes: &[u8]) -> Result<Rgba8, String> {
    let d3d = be32(bytes, 0x1C)? as usize;
    let size = be32(bytes, 0x20)? as usize;
    let fetch = d3d + 28;
    let f: Vec<u32> = (0..6).map(|i| be32(bytes, fetch + 4 * i)).collect::<Result<_, _>>()?;
    let tiled = f[0] >> 31 != 0;
    let pitch_texels = ((f[0] >> 22) & 0x1FF) * 32;
    let format = f[1] & 0x3F;
    let endian = (f[1] >> 6) & 3;
    let width = (f[2] & 0x1FFF) + 1;
    let height = ((f[2] >> 13) & 0x1FFF) + 1;
    let packed = f[5] >> 11 & 1 != 0;
    if format != 0x14 {
        return Err(format!("texture format {format:#x} is not decoded yet (only DXT4/5)"));
    }
    if endian != 1 {
        return Err(format!("texture endian mode {endian} is not decoded yet"));
    }
    if !tiled {
        return Err("linear (untiled) textures are not decoded yet".into());
    }
    let data = bytes.get(bytes.len().checked_sub(size).ok_or("texture data size past the file")?..).unwrap_or(&[]);
    let (ox, oy) = if packed { packed_offset(width, height) } else { (0, 0) };
    let (bw, bh) = (width.div_ceil(4), height.div_ceil(4));
    let pitch = pitch_texels.max(width) / 4;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for by in 0..bh {
        for bx in 0..bw {
            let at = tiled_offset(bx + ox / 4, by + oy / 4, pitch, 4) as usize;
            let raw = data.get(at..at + 16).ok_or("texture block past the data")?;
            let mut blk = [0u8; 16];
            for i in (0..16).step_by(2) {
                blk[i] = raw[i + 1];
                blk[i + 1] = raw[i];
            }
            for (i, px) in dxt5_block(&blk).iter().enumerate() {
                let (x, y) = (bx * 4 + i as u32 % 4, by * 4 + i as u32 / 4);
                if x < width && y < height {
                    let o = ((y * width + x) * 4) as usize;
                    pixels[o..o + 4].copy_from_slice(px);
                }
            }
        }
    }
    Ok(Rgba8 { width, height, pixels })
}

/// Load the `.img` named `name` (its full-name checksum) from an archive.
pub fn load_img(pak_path: &Path, name: &str) -> Result<Rgba8, String> {
    let headers = std::fs::read(pak_path).map_err(|e| format!("{}: {e}", pak_path.display()))?;
    let pab_path = pak_path.to_string_lossy().replace(".pak.xen", ".pab.xen");
    let pab = std::fs::read(&pab_path).ok();
    let (archive, data) =
        pak::parse_file(&headers, pab.as_deref()).map_err(|e| format!("{} was not recognised: {e:?}", pak_path.display()))?;
    let (key, img) = (qb_key(name), qb_key(".img"));
    let e = archive
        .entries
        .iter()
        .find(|e| e.type_key == img && e.full_name_key == key)
        .ok_or_else(|| format!("no texture {name} in {}", pak_path.display()))?;
    decode_img(data.get(e.offset..e.offset + e.size).ok_or("texture entry past the archive")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiled_offsets_match_the_reference_formula() {
        // Values from the research tool that decoded the retail textures.
        let cases = [
            (0, 0, 32, 0x0),
            (1, 0, 32, 0x20),
            (0, 1, 32, 0x10),
            (4, 0, 32, 0x200),
            (7, 7, 32, 0x1730),
            (8, 3, 32, 0x450),
            (31, 15, 32, 0x3770),
            (5, 20, 32, 0x1a20),
            (40, 2, 64, 0x4440),
            (63, 15, 64, 0x7770),
        ];
        for (x, y, pitch, want) in cases {
            assert_eq!(tiled_offset(x, y, pitch, 4), want, "block ({x}, {y})");
        }
    }

    /// A 16 x 32 DXT5 file laid out like `balancearrow_glow`: packed mips,
    /// the base level 16 texels into its tile.
    #[test]
    fn decodes_a_packed_dxt5_texture() {
        let mut file = vec![0u8; 0x1000 + 0x4000];
        file[0x1C..0x20].copy_from_slice(&0x28u32.to_be_bytes());
        file[0x20..0x24].copy_from_slice(&0x4000u32.to_be_bytes());
        let fetch = [0x8100_0002u32, 0x54, (31 << 13) | 15, 0xD10, 0, 0xA00];
        for (i, v) in fetch.iter().enumerate() {
            let at = 0x28 + 28 + 4 * i;
            file[at..at + 4].copy_from_slice(&v.to_be_bytes());
        }
        // Every block: alpha 255, colour 0 pure red (0xF800), indices 0.
        let mut block = [0u8; 16];
        block[0] = 255;
        block[1] = 255;
        block[8..10].copy_from_slice(&0xF800u16.to_le_bytes());
        let mut swapped = [0u8; 16];
        for i in (0..16).step_by(2) {
            swapped[i] = block[i + 1];
            swapped[i + 1] = block[i];
        }
        for by in 0..8 {
            for bx in 0..4 {
                let at = 0x1000 + tiled_offset(bx + 4, by, 32, 4) as usize;
                file[at..at + 16].copy_from_slice(&swapped);
            }
        }
        let img = decode_img(&file).unwrap();
        assert_eq!((img.width, img.height), (16, 32));
        assert!(img.pixels.chunks(4).all(|p| p == [255, 0, 0, 255]));
    }
}
