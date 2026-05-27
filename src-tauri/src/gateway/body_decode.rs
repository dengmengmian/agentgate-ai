//! Request-body decompression for incoming gateway requests.
//!
//! Codex.app (and other "production-grade" HTTP clients that treat the
//! gateway as a real OpenAI endpoint, e.g. when `requires_openai_auth =
//! true`) compresses request bodies with gzip or deflate by default. The
//! axum `String` extractor would then explode with
//! `Request body didn't contain valid UTF-8: invalid utf-8 sequence of 1
//! bytes from index 1` — the gzip magic header (`1f 8b ...`) failing UTF-8
//! decoding.
//!
//! This module replaces the `String` extractor with a `Bytes`-then-decode
//! pattern: handlers take `body: Bytes`, then call `decode(headers, body)`
//! to get a `String` honouring whatever `Content-Encoding` the client
//! advertised.

use crate::errors::AppError;
use axum::http::HeaderMap;
use bytes::Bytes;
use std::io::Read;

/// Decode the request body, honouring `Content-Encoding`. Returns the
/// decoded UTF-8 string ready for JSON parsing. Identity / absent encoding
/// is the common case and short-circuits to a single allocation.
///
/// Supported encodings: gzip / x-gzip / deflate / br / zstd / identity.
/// Modern clients (Codex.app, ChatGPT desktop) default to zstd; older /
/// generic ones use gzip or deflate. Brotli covers some browser-style
/// stacks that may show up via embedded webviews.
pub fn decode(headers: &HeaderMap, body: Bytes) -> Result<String, AppError> {
    let encoding = headers
        .get(axum::http::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    // Strip optional " ; q=..." weights some clients append.
    let primary = encoding
        .split(',')
        .next()
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    let decoded: Vec<u8> = match primary.as_str() {
        "" | "identity" => body.to_vec(),
        "gzip" | "x-gzip" => decompress_gzip(&body)?,
        "deflate" => decompress_deflate(&body)?,
        "br" => decompress_brotli(&body)?,
        "zstd" => decompress_zstd(&body)?,
        // Some clients chain encodings ("gzip, br" etc.). Try each in
        // order — almost always single-encoding in practice.
        multi if multi.contains("gzip") => decompress_gzip(&body)?,
        multi if multi.contains("deflate") => decompress_deflate(&body)?,
        multi if multi.contains("br") => decompress_brotli(&body)?,
        multi if multi.contains("zstd") => decompress_zstd(&body)?,
        other => {
            return Err(AppError::new(
                "UNSUPPORTED_CONTENT_ENCODING",
                format!("Request body uses unsupported Content-Encoding: {other}"),
            ));
        }
    };

    String::from_utf8(decoded).map_err(|e| {
        AppError::new(
            "INVALID_REQUEST_BODY",
            format!("Decoded body is not valid UTF-8: {e}"),
        )
    })
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut out = Vec::with_capacity(data.len() * 2);
    decoder.read_to_end(&mut out).map_err(|e| {
        AppError::new("GZIP_DECODE_FAILED", format!("Failed to decompress gzip body: {e}"))
    })?;
    Ok(out)
}

fn decompress_deflate(data: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = flate2::read::DeflateDecoder::new(data);
    let mut out = Vec::with_capacity(data.len() * 2);
    decoder.read_to_end(&mut out).map_err(|e| {
        AppError::new("DEFLATE_DECODE_FAILED", format!("Failed to decompress deflate body: {e}"))
    })?;
    Ok(out)
}

fn decompress_brotli(data: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = brotli::Decompressor::new(data, 4096);
    let mut out = Vec::with_capacity(data.len() * 3);
    decoder.read_to_end(&mut out).map_err(|e| {
        AppError::new("BROTLI_DECODE_FAILED", format!("Failed to decompress brotli body: {e}"))
    })?;
    Ok(out)
}

fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = zstd::stream::Decoder::new(data).map_err(|e| {
        AppError::new("ZSTD_DECODE_FAILED", format!("Failed to init zstd decoder: {e}"))
    })?;
    let mut out = Vec::with_capacity(data.len() * 3);
    decoder.read_to_end(&mut out).map_err(|e| {
        AppError::new("ZSTD_DECODE_FAILED", format!("Failed to decompress zstd body: {e}"))
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use flate2::write::{DeflateEncoder, GzEncoder};
    use flate2::Compression;
    use std::io::Write;

    fn hdrs(encoding: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(enc) = encoding {
            h.insert(
                HeaderName::from_static("content-encoding"),
                HeaderValue::from_str(enc).unwrap(),
            );
        }
        h
    }

    #[test]
    fn passes_through_uncompressed_body() {
        let body = Bytes::from_static(b"{\"hello\":\"world\"}");
        let out = decode(&hdrs(None), body).unwrap();
        assert_eq!(out, r#"{"hello":"world"}"#);
    }

    #[test]
    fn passes_through_identity_encoding() {
        let body = Bytes::from_static(b"plain text");
        let out = decode(&hdrs(Some("identity")), body).unwrap();
        assert_eq!(out, "plain text");
    }

    #[test]
    fn decompresses_gzip_body() {
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(br#"{"model":"gpt-4","messages":[]}"#).unwrap();
        let compressed = Bytes::from(e.finish().unwrap());
        let out = decode(&hdrs(Some("gzip")), compressed).unwrap();
        assert_eq!(out, r#"{"model":"gpt-4","messages":[]}"#);
    }

    #[test]
    fn decompresses_x_gzip_alias() {
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(b"hello").unwrap();
        let compressed = Bytes::from(e.finish().unwrap());
        let out = decode(&hdrs(Some("x-gzip")), compressed).unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn decompresses_deflate_body() {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
        e.write_all(b"deflate payload").unwrap();
        let compressed = Bytes::from(e.finish().unwrap());
        let out = decode(&hdrs(Some("deflate")), compressed).unwrap();
        assert_eq!(out, "deflate payload");
    }

    #[test]
    fn case_insensitive_encoding_header() {
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(b"x").unwrap();
        let compressed = Bytes::from(e.finish().unwrap());
        let out = decode(&hdrs(Some("GZIP")), compressed).unwrap();
        assert_eq!(out, "x");
    }

    #[test]
    fn picks_first_recognised_in_chained_encoding() {
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(b"chained").unwrap();
        let compressed = Bytes::from(e.finish().unwrap());
        let out = decode(&hdrs(Some("gzip, br")), compressed).unwrap();
        assert_eq!(out, "chained");
    }

    #[test]
    fn decompresses_zstd_body() {
        let payload = br#"{"model":"gpt-4","input":"hi"}"#;
        let compressed = Bytes::from(zstd::stream::encode_all(&payload[..], 3).unwrap());
        let out = decode(&hdrs(Some("zstd")), compressed).unwrap();
        assert_eq!(out, r#"{"model":"gpt-4","input":"hi"}"#);
    }

    #[test]
    fn decompresses_brotli_body() {
        let payload = b"brotli payload";
        let mut compressed = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 4, 22);
            writer.write_all(payload).unwrap();
        }
        let out = decode(&hdrs(Some("br")), Bytes::from(compressed)).unwrap();
        assert_eq!(out, "brotli payload");
    }

    #[test]
    fn rejects_unknown_encoding() {
        let body = Bytes::from_static(b"x");
        let err = decode(&hdrs(Some("xpress")), body).unwrap_err();
        assert_eq!(err.code, "UNSUPPORTED_CONTENT_ENCODING");
    }

    #[test]
    fn strips_quality_factors_from_encoding_header() {
        // Some clients append "; q=1.0" quality factors.
        let payload = br#"{"x":1}"#;
        let compressed = Bytes::from(zstd::stream::encode_all(&payload[..], 3).unwrap());
        let out = decode(&hdrs(Some("zstd; q=1.0")), compressed).unwrap();
        assert_eq!(out, r#"{"x":1}"#);
    }

    #[test]
    fn surfaces_gzip_decoder_error_on_bad_data() {
        // Random non-gzip bytes labelled as gzip → decoder error, not panic.
        let body = Bytes::from_static(b"not gzip");
        let err = decode(&hdrs(Some("gzip")), body).unwrap_err();
        assert_eq!(err.code, "GZIP_DECODE_FAILED");
    }

    #[test]
    fn surfaces_utf8_error_after_successful_decompression() {
        // gzipped non-UTF-8 bytes → decompresses, then fails the UTF-8 step.
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(&[0xff, 0xfe, 0xfd]).unwrap();
        let compressed = Bytes::from(e.finish().unwrap());
        let err = decode(&hdrs(Some("gzip")), compressed).unwrap_err();
        assert_eq!(err.code, "INVALID_REQUEST_BODY");
    }
}
