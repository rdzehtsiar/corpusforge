// SPDX-License-Identifier: Apache-2.0

//! Deterministic master seed parsing and display.

use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::Path;
use std::str::FromStr;

use crate::{CorpusForgeError, Result};

const SEED_BYTES: usize = 32;
const INTEGER_SEED_LABEL: &[u8] = b"corpusforge.master_seed.integer.v1\0";
const HEX_PREFIX: &str = "hex:";

/// A canonical 32-byte master seed used to derive deterministic streams.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MasterSeed([u8; SEED_BYTES]);

impl MasterSeed {
    /// Creates a master seed from raw seed bytes.
    pub const fn from_bytes(bytes: [u8; SEED_BYTES]) -> Self {
        Self(bytes)
    }

    /// Reads a master seed from a file that must contain exactly 32 bytes.
    pub fn from_seed_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path)?;
        let seed_bytes: [u8; SEED_BYTES] = bytes.try_into().map_err(|bytes: Vec<u8>| {
            CorpusForgeError::invalid_seed(format!(
                "seed file '{}' must contain exactly 32 bytes, found {}",
                path.display(),
                bytes.len()
            ))
        })?;

        Ok(Self(seed_bytes))
    }

    /// Returns the seed bytes for deterministic stream derivation.
    pub const fn as_bytes(&self) -> &[u8; SEED_BYTES] {
        &self.0
    }

    fn from_integer_text(text: &str) -> Result<Self> {
        if text.is_empty() {
            return Err(CorpusForgeError::invalid_seed("seed must not be empty"));
        }

        if !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(CorpusForgeError::invalid_seed(
                "integer seed must contain only ASCII decimal digits",
            ));
        }

        let canonical = canonical_decimal_ascii(text);
        let mut hasher = blake3::Hasher::new();
        hasher.update(INTEGER_SEED_LABEL);
        hasher.update(canonical.as_bytes());

        Ok(Self(*hasher.finalize().as_bytes()))
    }

    fn from_hex_text(text: &str) -> Result<Self> {
        if text.len() != SEED_BYTES * 2 {
            return Err(CorpusForgeError::invalid_seed(format!(
                "hex seed must contain exactly 64 hex characters, found {}",
                text.len()
            )));
        }

        let mut bytes = [0_u8; SEED_BYTES];
        for (index, chunk) in text.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_nibble(chunk[0]).ok_or_else(|| invalid_hex_character(chunk[0]))?;
            let low = hex_nibble(chunk[1]).ok_or_else(|| invalid_hex_character(chunk[1]))?;
            bytes[index] = (high << 4) | low;
        }

        Ok(Self(bytes))
    }
}

impl Display for MasterSeed {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }

        Ok(())
    }
}

impl FromStr for MasterSeed {
    type Err = CorpusForgeError;

    fn from_str(text: &str) -> Result<Self> {
        if let Some(hex) = text.strip_prefix(HEX_PREFIX) {
            Self::from_hex_text(hex)
        } else {
            Self::from_integer_text(text)
        }
    }
}

fn canonical_decimal_ascii(text: &str) -> &str {
    let trimmed = text.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid_hex_character(byte: u8) -> CorpusForgeError {
    CorpusForgeError::invalid_seed(format!(
        "hex seed contains non-hex character '{}'",
        char::from(byte)
    ))
}

#[cfg(test)]
mod tests {
    use super::MasterSeed;
    use crate::CorpusForgeError;
    use std::fs;
    use std::path::PathBuf;
    use std::str::FromStr;

    #[test]
    fn integer_seed_expansion_is_stable_and_canonical() {
        let seed = parse_seed("42");
        let seed_with_leading_zeroes = parse_seed("00042");

        assert_eq!(seed, seed_with_leading_zeroes);
        assert_eq!(
            seed.to_string(),
            "9c6097779aae54171352d8396b11d89d5a5ee12eaccb84f6f05d5bfe6e23c5bb"
        );
    }

    #[test]
    fn hex_seed_accepts_lowercase_and_uppercase() {
        let lowercase =
            parse_seed("hex:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let uppercase =
            parse_seed("hex:000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F");

        assert_eq!(lowercase, uppercase);
        assert_eq!(
            lowercase.as_bytes(),
            &[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31,
            ]
        );
    }

    #[test]
    fn rejects_invalid_hex_length() {
        let error = parse_seed_error("hex:abc");

        assert_invalid_seed_contains(error, "exactly 64 hex characters");
    }

    #[test]
    fn rejects_invalid_hex_content() {
        let error = parse_seed_error(
            "hex:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1g",
        );

        assert_invalid_seed_contains(error, "non-hex character");
    }

    #[test]
    fn rejects_invalid_file_length() {
        let path = temp_seed_path("short-seed");
        let write_result = fs::write(&path, [1_u8, 2, 3]);
        assert!(write_result.is_ok(), "failed to write temp seed file");

        let result = MasterSeed::from_seed_file(&path);
        let remove_result = fs::remove_file(&path);
        assert!(remove_result.is_ok(), "failed to remove temp seed file");

        let error = match result {
            Ok(seed) => panic!("expected invalid file length, got seed {seed}"),
            Err(error) => error,
        };

        assert_invalid_seed_contains(error, "exactly 32 bytes");
    }

    #[test]
    fn seed_file_accepts_exactly_32_bytes() {
        let path = temp_seed_path("exact-seed");
        let bytes = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ];
        let write_result = fs::write(&path, bytes);
        assert!(write_result.is_ok(), "failed to write temp seed file");

        let result = MasterSeed::from_seed_file(&path);
        let remove_result = fs::remove_file(&path);
        assert!(remove_result.is_ok(), "failed to remove temp seed file");

        let seed = match result {
            Ok(seed) => seed,
            Err(error) => panic!("failed to read exact-length seed file: {error}"),
        };

        assert_eq!(seed.as_bytes(), &bytes);
    }

    #[test]
    fn display_formats_stable_lowercase_hex() {
        let seed =
            parse_seed("hex:ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD");

        assert_eq!(
            seed.to_string(),
            "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd"
        );
    }

    fn parse_seed(text: &str) -> MasterSeed {
        match MasterSeed::from_str(text) {
            Ok(seed) => seed,
            Err(error) => panic!("failed to parse seed '{text}': {error}"),
        }
    }

    fn parse_seed_error(text: &str) -> CorpusForgeError {
        match MasterSeed::from_str(text) {
            Ok(seed) => panic!("expected invalid seed for '{text}', got {seed}"),
            Err(error) => error,
        }
    }

    fn assert_invalid_seed_contains(error: CorpusForgeError, expected: &str) {
        assert_eq!(error.category(), "invalid_seed");
        assert!(
            error.to_string().contains(expected),
            "expected '{error}' to contain '{expected}'"
        );
    }

    fn temp_seed_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "corpusforge-core-seed-test-{name}-{}.bin",
            std::process::id()
        ));
        path
    }
}
