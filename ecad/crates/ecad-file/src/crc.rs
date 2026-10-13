//! CRC-32C (Castagnoli), the frame checksum (proposal §20.10).

const POLY: u32 = 0x82F6_3B78; // reflected 0x1EDC6F41

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ POLY } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static TABLE: [u32; 256] = table();

/// CRC-32C of `bytes`.
///
/// # Example
/// ```rust
/// assert_eq!(ecad_file::crc32c(b"123456789"), 0xE306_9283);
/// ```
pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        let i = usize::from((c as u8) ^ b);
        c = TABLE.get(i).copied().unwrap_or(0) ^ (c >> 8);
    }
    !c
}
