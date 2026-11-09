//! Compression of stored rows and chunks (EEP-0005 §4): Zstandard when it makes them smaller, with
//! a flag byte saying which. Hashes and chain values are always over the uncompressed bytes.
//!
//! Native builds compress and decompress with the `zstd` crate (libzstd, level 1); the wasm
//! reader decodes with `ruzstd` (pure Rust), so a browser build needs no C toolchain (overseer,
//! 2026-11-09). Both read the same standard Zstandard frames: the format does not depend on
//! which wrote it. Decoding is capped, so a hostile file cannot make a reader allocate without
//! bound.

use std::io::Read;

/// Flag byte: the body is stored as it is.
pub const RAW: u8 = 0;
/// Flag byte: the body is a Zstandard frame.
pub const ZSTD: u8 = 1;

/// Largest uncompressed body read (64 MiB, as E3 §18 caps a payload).
pub const MAX_BODY: usize = 64 << 20;

/// The Zstandard level native builds write.
pub const LEVEL: i32 = 1;

/// A Zstandard frame of `bytes`.
#[cfg(not(target_arch = "wasm32"))]
fn compress(bytes: &[u8]) -> Option<Vec<u8>> {
    zstd::bulk::compress(bytes, LEVEL).ok()
}

/// A Zstandard frame of `bytes` (the wasm build writes only into memory).
#[cfg(target_arch = "wasm32")]
fn compress(bytes: &[u8]) -> Option<Vec<u8>> {
    Some(ruzstd::encoding::compress_to_vec(bytes, ruzstd::encoding::CompressionLevel::Fastest))
}

/// `bytes` as a stored body: the flag and the bytes, compressed when that is smaller.
pub fn pack(bytes: &[u8]) -> (u8, Vec<u8>) {
    if bytes.len() >= 64
        && let Some(z) = compress(bytes)
        && z.len() < bytes.len()
    {
        return (ZSTD, z);
    }
    (RAW, bytes.to_vec())
}

/// Read all of `r`, refusing more than [`MAX_BODY`] bytes.
fn read_capped(r: impl Read) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    r.take(MAX_BODY as u64 + 1).read_to_end(&mut out).map_err(|e| format!("damaged compressed body: {e}"))?;
    if out.len() > MAX_BODY {
        return Err(format!("a compressed body expands to more than {MAX_BODY} bytes"));
    }
    Ok(out)
}

/// A Zstandard frame decoded with `ruzstd` (the wasm reader's decoder; also used natively by the
/// test that keeps the two decoders in agreement).
///
/// # Errors
/// A damaged frame, or more than [`MAX_BODY`] bytes.
pub fn decode_ruzstd(body: &[u8]) -> Result<Vec<u8>, String> {
    let d = ruzstd::decoding::StreamingDecoder::new(body).map_err(|e| format!("damaged compressed body: {e}"))?;
    read_capped(d)
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    /// One decompression context per thread, reused: making one per row costs more than the row
    /// (a history is read row by row).
    static DCTX: std::cell::RefCell<Option<zstd::bulk::Decompressor<'static>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(not(target_arch = "wasm32"))]
fn decode(body: &[u8]) -> Result<Vec<u8>, String> {
    let damaged = |e: &dyn std::fmt::Display| format!("damaged compressed body: {e}");
    match zstd::zstd_safe::get_frame_content_size(body) {
        // The frame says its size (every frame this build writes does): decode in one call.
        Ok(Some(n)) => {
            let n = usize::try_from(n)
                .ok()
                .filter(|n| *n <= MAX_BODY)
                .ok_or_else(|| format!("a compressed body expands to more than {MAX_BODY} bytes"))?;
            let run = |d: &mut zstd::bulk::Decompressor<'static>| d.decompress(body, n).map_err(|e| damaged(&e));
            let out = DCTX.with(|cell| match cell.try_borrow_mut() {
                Ok(mut slot) => {
                    if slot.is_none() {
                        *slot = zstd::bulk::Decompressor::new().ok();
                    }
                    match slot.as_mut() {
                        Some(d) => run(d),
                        None => Err("no decompression context".to_string()),
                    }
                }
                Err(_) => zstd::bulk::Decompressor::new().map_err(|e| damaged(&e)).and_then(|mut d| run(&mut d)),
            })?;
            if out.len() != n {
                return Err(damaged(&"its size differs from its header"));
            }
            Ok(out)
        }
        // A frame without its size (another writer): stream it, capped.
        Ok(None) => read_capped(zstd::stream::read::Decoder::new(body).map_err(|e| damaged(&e))?),
        Err(_) => Err(damaged(&"not a Zstandard frame")),
    }
}

#[cfg(target_arch = "wasm32")]
fn decode(body: &[u8]) -> Result<Vec<u8>, String> {
    decode_ruzstd(body)
}

/// The uncompressed bytes of a stored body.
///
/// # Errors
/// An unknown flag, a damaged Zstandard frame, or a body larger than [`MAX_BODY`].
pub fn unpack(flag: u8, body: &[u8]) -> Result<Vec<u8>, String> {
    match flag {
        RAW if body.len() <= MAX_BODY => Ok(body.to_vec()),
        RAW => Err(format!("a stored body is larger than {MAX_BODY} bytes")),
        ZSTD => decode(body),
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
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let noise: Vec<u8> = (0..1000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
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

    #[test]
    fn the_wasm_decoder_reads_what_native_builds_write() {
        // Rows and chunks of the kinds the container stores, from tiny to a few hundred KB.
        let mut x = 7u64;
        let mut samples: Vec<Vec<u8>> = vec![
            br#"{"type":"tx","seq":1,"ops":[{"op":"AddPage","v":1}],"prev":"00"}"#.repeat(3),
            br#"{"items":{},"page":{"title":"1"},"representations":{}}"#.repeat(40),
        ];
        let mut mixed = Vec::new();
        for i in 0..200_000u32 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            mixed.extend_from_slice(if x.is_multiple_of(3) { b"\"conductor\":" } else { b"[1.5,-2.25]," });
            if i.is_multiple_of(97) {
                mixed.push((x >> 40) as u8);
            }
        }
        samples.push(mixed);
        for s in samples {
            let (flag, body) = pack(&s);
            assert_eq!(flag, ZSTD, "{}", s.len());
            assert_eq!(decode_ruzstd(&body).unwrap(), s, "ruzstd reads the native frame");
            assert_eq!(unpack(flag, &body).unwrap(), s);
        }
    }
}
