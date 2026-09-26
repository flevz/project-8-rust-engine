//! Neversoft QB name checksums.
//!
//! CONFIRMED (Project8Recomp `guest_probe.h`, verified there against 118,762
//! name/checksum pairs from the disc's debug data): CRC-32 with the reflected
//! polynomial 0xEDB88320 and initial value 0xFFFFFFFF, over the name
//! lower-cased with `/` replaced by `\`, and **no final complement**.

const fn table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static TABLE: [u32; 256] = table();

/// The QB checksum ("QBKey") of a name.
pub fn qb_key(name: &str) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in name.bytes() {
        let b = match b.to_ascii_lowercase() {
            b'/' => b'\\',
            b => b,
        };
        crc = TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::qb_key;

    /// Every pair below is quoted in Project8Recomp's `dev_cheats.h`, where it
    /// is used successfully against the retail executable.
    #[test]
    fn matches_retail_checksums() {
        assert_eq!(qb_key("string"), 0x6141_4D56);
        assert_eq!(qb_key("level"), 0x6515_33EC);
        assert_eq!(qb_key("nodename"), 0x9F92_BA78);
        assert_eq!(qb_key("load_z_funpark"), 0x2349_A9C0);
        assert_eq!(qb_key("not_from_levelselect_menu"), 0x856E_853E);
    }

    #[test]
    fn is_case_and_slash_insensitive() {
        assert_eq!(qb_key("Levels/Z_Funpark"), qb_key("levels\\z_funpark"));
    }
}
