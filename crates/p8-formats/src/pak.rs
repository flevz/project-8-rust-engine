//! Neversoft `.pak.xen` / `.pab.xen` archives.
//!
//! Confidence:
//! - CONFIRMED: Project 8 ships data in `.pak.xen` containers
//!   (Project8Recomp `dev_cheats.h` names `dbg.pak.xen`).
//! - LIKELY: the entry layout below, which later Neversoft engine titles use
//!   and which is widely documented by their modding communities. It has
//!   **not yet been checked against Project 8 files**. [`Pak::parse`] therefore
//!   validates every entry and reports clearly when the hypothesis fails, and
//!   `p8-inspect` exists to test it on a real installation.
//!
//! Hypothesised layout, big-endian, one 32-byte header per entry:
//!
//! | offset | field |
//! | --- | --- |
//! | 0 | type key (QB key of the extension, for example `.qb`) |
//! | 4 | data offset, relative to the start of this header |
//! | 8 | data size |
//! | 12 | archive full-name key |
//! | 16 | full-name key |
//! | 20 | short-name key |
//! | 24 | parent key |
//! | 28 | flags; bit 0x20 means a 160-byte name string follows the header |
//!
//! CONFIRMED on retail `dbg.pak.xen` and `dbgq.pak.xen`: the layout above,
//! with offsets relative to each header, entries aligned to 16 bytes, and the
//! whole file compressed as a raw DEFLATE stream (see [`decompress`]).
//!
//! The list ends with an entry whose type is `.last` or `last`. When a sibling
//! `.pab.xen` exists, the headers live in the `.pak.xen` and the data in the
//! `.pab.xen`; offsets then address the two files concatenated.
use crate::qb_key;

pub const NAME_FLAG: u32 = 0x20;
const HEADER: usize = 32;
const NAME_LEN: usize = 160;
const MAX_ENTRIES: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    Big,
    Little,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub type_key: u32,
    /// Absolute offset into the (pak + pab) data.
    pub offset: usize,
    pub size: usize,
    pub full_name_key: u32,
    pub name_key: u32,
    pub parent_key: u32,
    pub flags: u32,
    /// Embedded name, present when `flags & NAME_FLAG`.
    pub name: Option<String>,
}

#[derive(Debug)]
pub struct Pak {
    pub endian: Endian,
    pub entries: Vec<Entry>,
    /// True if the terminating `.last` entry was found.
    pub terminated: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PakError {
    TooShort,
    /// The first header is not plausible in either byte order.
    NotRecognised,
}

fn read_u32(data: &[u8], at: usize, endian: Endian) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(match endian {
        Endian::Big => u32::from_be_bytes(bytes),
        Endian::Little => u32::from_le_bytes(bytes),
    })
}

fn is_last(key: u32) -> bool {
    key == qb_key(".last") || key == qb_key("last")
}

impl Pak {
    /// Parse headers from `headers` (the `.pak.xen`). `total_len` is the length
    /// of pak plus pab data, used to validate offsets.
    pub fn parse(headers: &[u8], total_len: usize) -> Result<Pak, PakError> {
        if headers.len() < HEADER {
            return Err(PakError::TooShort);
        }
        let big = Self::parse_as(headers, total_len, Endian::Big);
        let little = Self::parse_as(headers, total_len, Endian::Little);
        let best = if little.score() > big.score() {
            little
        } else {
            big
        };
        // A single stray "entry" is not evidence; require a terminator or several entries.
        if !(best.terminated || best.entries.len() >= 4) {
            return Err(PakError::NotRecognised);
        }
        Ok(best)
    }

    fn score(&self) -> usize {
        self.entries.len() * 2 + usize::from(self.terminated) * 3
    }

    fn parse_as(headers: &[u8], total_len: usize, endian: Endian) -> Pak {
        let mut entries = Vec::new();
        let mut at = 0usize;
        let mut terminated = false;
        while entries.len() < MAX_ENTRIES {
            let mut f = [0u32; 8];
            if (0..8).any(|i| match read_u32(headers, at + i * 4, endian) {
                Some(v) => {
                    f[i] = v;
                    false
                }
                None => true,
            }) {
                break;
            }
            let [type_key, rel, size, _pak_key, full, name_key, parent, flags] = f;
            if is_last(type_key) {
                terminated = true;
                break;
            }
            let offset = at.checked_add(rel as usize);
            let valid = type_key != 0
                && offset.is_some_and(|o| {
                    o.checked_add(size as usize)
                        .is_some_and(|end| end <= total_len)
                })
                && flags & !0xFFFF == 0;
            if !valid {
                break;
            }
            let mut next = at + HEADER;
            let mut name = None;
            if flags & NAME_FLAG != 0 {
                let Some(raw) = headers.get(next..next + NAME_LEN) else {
                    break;
                };
                let end = raw.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
                name = std::str::from_utf8(&raw[..end]).ok().map(str::to_owned);
                next += NAME_LEN;
            }
            entries.push(Entry {
                type_key,
                offset: offset.unwrap(),
                size: size as usize,
                full_name_key: full,
                name_key,
                parent_key: parent,
                flags,
                name,
            });
            at = next;
        }
        Pak {
            endian,
            entries,
            terminated,
        }
    }
}

/// Retail archives are a single raw DEFLATE stream (no zlib header).
/// Returns the input unchanged if it is not compressed that way.
pub fn decompress(bytes: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut decoder = flate2::read::DeflateDecoder::new(bytes);
    match decoder.read_to_end(&mut out) {
        Ok(_) if !out.is_empty() => out,
        _ => bytes.to_vec(),
    }
}

/// Parse a file as stored on disc, decompressing first when needed.
pub fn parse_file(headers: &[u8], pab: Option<&[u8]>) -> Result<(Pak, Vec<u8>), PakError> {
    let plain = |b: &[u8]| -> (Vec<u8>, usize) {
        let mut data = b.to_vec();
        let len = data.len();
        if let Some(p) = pab {
            data.extend_from_slice(p);
        }
        (data, len)
    };
    let (data, _) = plain(headers);
    if let Ok(pak) = Pak::parse(headers, data.len()) {
        return Ok((pak, data));
    }
    let inflated = decompress(headers);
    let pab = pab.map(decompress);
    let mut data = inflated.clone();
    if let Some(p) = &pab {
        data.extend_from_slice(p);
    }
    let pak = Pak::parse(&inflated, data.len())?;
    Ok((pak, data))
}

/// Common Neversoft extensions, for naming `type_key`s in reports.
pub const KNOWN_EXTENSIONS: &[&str] = &[
    ".qb", ".mqb", ".sqb", ".nqb", ".qs", ".qs.en", ".tex", ".img", ".pimg", ".stex", ".scn",
    ".mdl", ".skin", ".geo", ".col", ".ska", ".ske", ".anim", ".cam", ".fam", ".fnt", ".dbg",
    ".rag", ".clt", ".nav", ".rnb", ".rnb_lvl", ".rnb_mdl", ".tvx", ".cif", ".shd", ".ptl", ".wav",
    ".snd", ".bnk", ".xml", ".txt", ".trg", ".oba", ".obj", ".pak", ".pab", ".bin", ".dat", ".ps",
    ".vs", ".fx", ".last", "last",
];

/// Name a type key if it is a known extension.
pub fn extension_name(key: u32) -> Option<&'static str> {
    KNOWN_EXTENSIONS.iter().copied().find(|e| qb_key(e) == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(out: &mut Vec<u8>, ty: &str, rel: u32, size: u32, flags: u32, name: Option<&str>) {
        for v in [
            qb_key(ty),
            rel,
            size,
            0xAAAA_AAAA,
            0x1111_1111,
            0x2222_2222,
            0,
            flags,
        ] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        if let Some(n) = name {
            let mut raw = [0u8; NAME_LEN];
            raw[..n.len()].copy_from_slice(n.as_bytes());
            out.extend_from_slice(&raw);
        }
    }

    #[test]
    fn parses_synthetic_big_endian_archive_with_names_and_terminator() {
        let mut pak = Vec::new();
        header(&mut pak, ".qb", 512, 4, 0, None);
        header(
            &mut pak,
            ".tex",
            516 - 32,
            8,
            NAME_FLAG,
            Some("textures/a.tex"),
        );
        header(&mut pak, ".last", 0, 0, 0, None);
        pak.resize(524, 0);
        let parsed = Pak::parse(&pak, pak.len()).unwrap();
        assert_eq!(parsed.endian, Endian::Big);
        assert!(parsed.terminated);
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[1].offset, 516);
        assert_eq!(parsed.entries[1].name.as_deref(), Some("textures/a.tex"));
        assert_eq!(extension_name(parsed.entries[0].type_key), Some(".qb"));
    }

    #[test]
    fn compressed_archives_are_inflated_before_parsing() {
        use std::io::Write;
        let mut pak = Vec::new();
        header(&mut pak, ".qb", 512, 4, 0, None);
        header(&mut pak, ".last", 0, 0, 0, None);
        pak.resize(516, 7);
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&pak).unwrap();
        let packed = enc.finish().unwrap();
        let (parsed, data) = parse_file(&packed, None).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(&data[512..516], &[7, 7, 7, 7]);
    }

    #[test]
    fn rejects_unrelated_data() {
        let noise: Vec<u8> = (0..1024u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        assert!(Pak::parse(&noise, noise.len()).is_err());
    }

    #[test]
    fn offsets_may_point_into_a_separate_pab() {
        let mut pak = Vec::new();
        header(&mut pak, ".scn", 1000, 16, 0, None);
        header(&mut pak, "last", 0, 0, 0, None);
        let parsed = Pak::parse(&pak, pak.len() + 2000).unwrap();
        assert_eq!(parsed.entries[0].offset, 1000);
    }
}
