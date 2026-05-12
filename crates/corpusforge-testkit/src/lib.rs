// SPDX-License-Identifier: Apache-2.0

//! Shared test utilities for CorpusForge workspace tests.

/// Shared deterministic seed bytes for unit tests that need a stable master seed.
pub const TEST_SEED_BYTES: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];

/// Formats bytes as lowercase hexadecimal for deterministic fixture assertions.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hex
}

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-testkit"
}

#[cfg(test)]
mod tests {
    use super::{bytes_to_hex, crate_name, TEST_SEED_BYTES};

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-testkit");
    }

    #[test]
    fn exposes_stable_test_seed_bytes() {
        assert_eq!(TEST_SEED_BYTES.len(), 32);
        assert_eq!(TEST_SEED_BYTES[0], 0);
        assert_eq!(TEST_SEED_BYTES[31], 31);
    }

    #[test]
    fn formats_lowercase_hex() {
        assert_eq!(bytes_to_hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }
}
