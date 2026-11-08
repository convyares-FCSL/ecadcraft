//! Compression of stored rows and parts (EEP-0005 §4): Zstandard when it makes them smaller, with
//! a flag byte saying which. Hashes and chain values are always over the uncompressed bytes.
//!
//! Zstandard is `ruzstd` (pure Rust; overseer, 2026-11-08), at its only implemented level,
//! `Fastest` (about zstd level 1). Decoding is capped, so a hostile file cannot make a reader
//! allocate without bound.

use std::io::Read;

use ruzstd::decoding::StreamingDecoder;
use ruzstd::encoding::{CompressionLevel, compress_to_vec};

/// Flag byte: the body is stored as it is.
pub const RAW: u8 = 0;
/// Flag byte: the body is a Zstandard frame.
pub const ZSTD: u8 = 1;

/// Largest uncompressed body read (64 MiB, as E3 §18 caps a payload).
pub const MAX_BODY: usize = 64 << 20;

/// `bytes` as a stored body: the flag and the bytes, compressed when that is smaller.
pub fn pack(bytes: &[u8]) -> (u8, Vec<u8>) {
    if bytes.len() >= 64 {
        let z = compress_to_vec(bytes, CompressionLevel::Fastest);
        if z.len() < bytes.len() {
            return (ZSTD, z);
        }
    }
    (RAW, bytes.to_vec())
}

/// The uncompressed bytes of a stored body.
///
/// # Errors
/// An unknown flag, a damaged Zstandard frame, or a body larger than [`MAX_BODY`].
pub fn unpack(flag: u8, body: &[u8]) -> Result<Vec<u8>, String> {
    match flag {
        RAW if body.len() <= MAX_BODY => Ok(body.to_vec()),
        RAW => Err(format!("a stored body is larger than {MAX_BODY} bytes")),
        ZSTD => {
            let decoder = StreamingDecoder::new(body).map_err(|e| format!("damaged compressed body: {e}"))?;
            let mut out = Vec::new();
            decoder
                .take(MAX_BODY as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|e| format!("damaged compressed body: {e}"))?;
            if out.len() > MAX_BODY {
                return Err(format!("a compressed body expands to more than {MAX_BODY} bytes"));
            }
            Ok(out)
        }
        other => Err(format!("unknown compression flag {other}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn small_or_incompressible_bodies_stay_raw_and_everything_round_trips() {
        let text = br#"{"type":"tx","ops":[{"op":"AddPage"},{"op":"AddPage"},{"op":"AddPage"},{"op":"AddPage"}]}"#;
        let noise: Vec<u8> = (0..1000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for (bytes, flag) in [(&b"tiny"[..], RAW), (&text[..], ZSTD), (&noise[..], RAW)] {
            let (f, body) = pack(bytes);
            assert_eq!(f, flag, "{}", bytes.len());
            assert_eq!(unpack(f, &body).unwrap(), bytes);
        }
    }

    #[test]
    fn damaged_or_unknown_bodies_are_refused_not_panicked() {
        let (f, mut body) = pack(&[b'a'; 4096]);
        assert_eq!(f, ZSTD);
        for i in 0..body.len() {
            body[i] ^= 0x5a;
            let _ = unpack(f, &body);
            body[i] ^= 0x5a;
        }
        assert!(unpack(f, &body[..body.len() / 2]).is_err());
        assert!(unpack(7, b"x").is_err());
    }
}
