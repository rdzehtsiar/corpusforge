// SPDX-License-Identifier: Apache-2.0

//! Placeholder crate for CorpusForge Unicode adversarial cases.

use corpusforge_core::rng::{DeterministicStream, DOMAIN_UNICODE};
use corpusforge_core::seed::MasterSeed;
use corpusforge_core::{CorpusForgeError, Result};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-unicode"
}

/// Unicode adversarial fixture families supported by the Milestone 4 model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnicodeMode {
    /// Extended grapheme cluster stress cases.
    Grapheme,
    /// Bidirectional text control stress cases.
    Bidi,
    /// Zero-width and invisible code point stress cases.
    ZeroWidth,
    /// Emoji sequence stress cases.
    Emoji,
    /// Unicode normalization stress cases.
    Normalization,
    /// Deterministic mixtures of valid Unicode stress families.
    Mixed,
    /// Raw byte cases that intentionally may not decode as UTF-8.
    InvalidUtf8,
}

impl UnicodeMode {
    /// Stable mode order used by diagnostics, fixtures, and tests.
    pub const ALL: [Self; 7] = [
        Self::Grapheme,
        Self::Bidi,
        Self::ZeroWidth,
        Self::Emoji,
        Self::Normalization,
        Self::Mixed,
        Self::InvalidUtf8,
    ];

    /// Returns the stable label used in profiles, diagnostics, and fixtures.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Grapheme => "grapheme",
            Self::Bidi => "bidi",
            Self::ZeroWidth => "zero-width",
            Self::Emoji => "emoji",
            Self::Normalization => "normalization",
            Self::Mixed => "mixed",
            Self::InvalidUtf8 => "invalid-utf8",
        }
    }

    /// Returns true when this mode is allowed at the selected output boundary.
    pub const fn is_supported_at(self, output_kind: UnicodeOutputKind) -> bool {
        !matches!(
            (self, output_kind),
            (Self::InvalidUtf8, UnicodeOutputKind::ValidText)
        )
    }
}

impl Display for UnicodeMode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl FromStr for UnicodeMode {
    type Err = CorpusForgeError;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "grapheme" => Ok(Self::Grapheme),
            "bidi" => Ok(Self::Bidi),
            "zero-width" => Ok(Self::ZeroWidth),
            "emoji" => Ok(Self::Emoji),
            "normalization" => Ok(Self::Normalization),
            "mixed" => Ok(Self::Mixed),
            "invalid-utf8" => Ok(Self::InvalidUtf8),
            _ => Err(CorpusForgeError::invalid_argument(format!(
                "unsupported Unicode mode `{text}`; expected one of: {}",
                stable_labels(&Self::ALL)
            ))),
        }
    }
}

/// Output boundary for Unicode fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnicodeOutputKind {
    /// Fixture output must be valid UTF-8 text.
    ValidText,
    /// Fixture output may contain arbitrary bytes, including invalid UTF-8.
    RawBytes,
}

impl UnicodeOutputKind {
    /// Stable output boundary order used by diagnostics, fixtures, and tests.
    pub const ALL: [Self; 2] = [Self::ValidText, Self::RawBytes];

    /// Returns the stable label used in profiles, diagnostics, and fixtures.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ValidText => "valid-text",
            Self::RawBytes => "raw-bytes",
        }
    }
}

impl Display for UnicodeOutputKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl FromStr for UnicodeOutputKind {
    type Err = CorpusForgeError;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "valid-text" => Ok(Self::ValidText),
            "raw-bytes" => Ok(Self::RawBytes),
            _ => Err(CorpusForgeError::invalid_argument(format!(
                "unsupported Unicode output kind `{text}`; expected one of: {}",
                stable_labels(&Self::ALL)
            ))),
        }
    }
}

/// Validated fixture request for a Unicode mode at a concrete output boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnicodeFixtureSpec {
    mode: UnicodeMode,
    output_kind: UnicodeOutputKind,
}

impl UnicodeFixtureSpec {
    /// Builds a validated Unicode fixture specification.
    pub fn new(mode: UnicodeMode, output_kind: UnicodeOutputKind) -> Result<Self> {
        validate_mode_output(mode, output_kind)?;

        Ok(Self { mode, output_kind })
    }

    /// Returns the requested Unicode fixture mode.
    pub const fn mode(self) -> UnicodeMode {
        self.mode
    }

    /// Returns the requested output boundary.
    pub const fn output_kind(self) -> UnicodeOutputKind {
        self.output_kind
    }
}

/// Validates that a Unicode mode can produce output at the requested boundary.
pub fn validate_mode_output(mode: UnicodeMode, output_kind: UnicodeOutputKind) -> Result<()> {
    if mode.is_supported_at(output_kind) {
        return Ok(());
    }

    Err(CorpusForgeError::invalid_argument(format!(
        "Unicode mode `{mode}` cannot be used with `{output_kind}` output because it may produce invalid UTF-8; use `raw-bytes` output or choose a valid-text mode"
    )))
}

const GRAPHEME_FIXTURES: &[&str] = &[
    "a\u{0301}",
    "\u{0915}\u{094D}\u{0937}\u{093F}",
    "\u{0BA8}\u{0BBF}",
    "\u{D55C}\u{AE00}",
    "Z\u{0351}\u{0357}\u{0323}",
];

const BIDI_FIXTURES: &[&str] = &[
    "abc\u{202E}fed\u{202C}",
    "left\u{2067}\u{05D9}\u{05DE}\u{05D9}\u{05DF}\u{2069}right",
    "\u{200F}rtl marker",
    "A\u{061C}+B",
    "start\u{202A}ltr\u{202C}end",
];

const ZERO_WIDTH_FIXTURES: &[&str] = &[
    "zero\u{200B}width",
    "join\u{200D}er",
    "word\u{2060}joiner",
    "bom\u{FEFF}inside",
    "soft\u{00AD}hyphen",
];

const EMOJI_FIXTURES: &[&str] = &[
    "\u{1F469}\u{200D}\u{1F4BB}",
    "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
    "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
    "\u{1F44D}\u{1F3FD}",
    "5\u{FE0F}\u{20E3}",
];

const NORMALIZATION_FIXTURES: &[&str] =
    &["\u{00E9}", "e\u{0301}", "\u{00C5}", "A\u{030A}", "\u{212B}"];

const VALID_TEXT_FAMILIES: &[&[&str]] = &[
    GRAPHEME_FIXTURES,
    BIDI_FIXTURES,
    ZERO_WIDTH_FIXTURES,
    EMOJI_FIXTURES,
    NORMALIZATION_FIXTURES,
];

const INVALID_UTF8_FIXTURES: &[&[u8]] = &[
    b"\x80",
    b"\xC0\xAF",
    b"\xE2\x28\xA1",
    b"\xF0\x28\x8C\x28",
    b"prefix\xED\xA0\x80suffix",
];

/// Generates deterministic valid UTF-8 Unicode adversarial text cases.
pub fn generate_valid_text(
    master_seed: &MasterSeed,
    mode: UnicodeMode,
    case_count: usize,
) -> Result<String> {
    validate_mode_output(mode, UnicodeOutputKind::ValidText)?;

    if case_count == 0 {
        return Ok(String::new());
    }

    let context = format!("valid-text/v1/{}", mode.label());
    let mut stream =
        DeterministicStream::from_seed_with_context(master_seed, DOMAIN_UNICODE, context);
    let mut output = String::new();

    for case_index in 0..case_count {
        if case_index > 0 {
            output.push('\n');
        }

        let fixture = sample_valid_text_fixture(mode, &mut stream)?;
        output.push_str(fixture);
    }

    Ok(output)
}

/// Generates deterministic raw-byte Unicode adversarial cases.
pub fn generate_raw_bytes(
    master_seed: &MasterSeed,
    mode: UnicodeMode,
    case_count: usize,
) -> Result<Vec<u8>> {
    validate_mode_output(mode, UnicodeOutputKind::RawBytes)?;

    if case_count == 0 {
        return Ok(Vec::new());
    }

    let context = format!("raw-bytes/v1/{}", mode.label());
    let mut stream =
        DeterministicStream::from_seed_with_context(master_seed, DOMAIN_UNICODE, context);
    let mut output = Vec::new();

    for case_index in 0..case_count {
        if case_index > 0 {
            output.push(b'\n');
        }

        let fixture = sample_raw_byte_fixture(mode, case_index, &mut stream)?;
        output.extend_from_slice(fixture);
    }

    Ok(output)
}

fn sample_valid_text_fixture(
    mode: UnicodeMode,
    stream: &mut DeterministicStream,
) -> Result<&'static str> {
    let fixtures = match mode {
        UnicodeMode::Grapheme => GRAPHEME_FIXTURES,
        UnicodeMode::Bidi => BIDI_FIXTURES,
        UnicodeMode::ZeroWidth => ZERO_WIDTH_FIXTURES,
        UnicodeMode::Emoji => EMOJI_FIXTURES,
        UnicodeMode::Normalization => NORMALIZATION_FIXTURES,
        UnicodeMode::Mixed => {
            let family_index = stream.usize_below(VALID_TEXT_FAMILIES.len())?;
            VALID_TEXT_FAMILIES[family_index]
        }
        UnicodeMode::InvalidUtf8 => unreachable!("invalid UTF-8 is rejected before sampling"),
    };

    let fixture_index = stream.usize_below(fixtures.len())?;
    Ok(fixtures[fixture_index])
}

fn sample_raw_byte_fixture(
    mode: UnicodeMode,
    case_index: usize,
    stream: &mut DeterministicStream,
) -> Result<&'static [u8]> {
    match mode {
        UnicodeMode::Grapheme
        | UnicodeMode::Bidi
        | UnicodeMode::ZeroWidth
        | UnicodeMode::Emoji
        | UnicodeMode::Normalization => Ok(sample_valid_text_fixture(mode, stream)?.as_bytes()),
        UnicodeMode::Mixed => sample_mixed_raw_byte_fixture(case_index, stream),
        UnicodeMode::InvalidUtf8 => sample_invalid_utf8_fixture(stream),
    }
}

fn sample_mixed_raw_byte_fixture(
    case_index: usize,
    stream: &mut DeterministicStream,
) -> Result<&'static [u8]> {
    let family_index = case_index % (VALID_TEXT_FAMILIES.len() + 1);

    if family_index == VALID_TEXT_FAMILIES.len() {
        return sample_invalid_utf8_fixture(stream);
    }

    let fixtures = VALID_TEXT_FAMILIES[family_index];
    let fixture_index = stream.usize_below(fixtures.len())?;
    Ok(fixtures[fixture_index].as_bytes())
}

fn sample_invalid_utf8_fixture(stream: &mut DeterministicStream) -> Result<&'static [u8]> {
    let fixture_index = stream.usize_below(INVALID_UTF8_FIXTURES.len())?;
    Ok(INVALID_UTF8_FIXTURES[fixture_index])
}

fn stable_labels<T: Copy + Display, const N: usize>(items: &[T; N]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        crate_name, generate_raw_bytes, generate_valid_text, validate_mode_output,
        UnicodeFixtureSpec, UnicodeMode, UnicodeOutputKind, BIDI_FIXTURES, EMOJI_FIXTURES,
        GRAPHEME_FIXTURES, NORMALIZATION_FIXTURES, VALID_TEXT_FAMILIES, ZERO_WIDTH_FIXTURES,
    };
    use corpusforge_core::seed::MasterSeed;
    use std::str::FromStr;

    const TEST_SEED: MasterSeed = MasterSeed::from_bytes([
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31,
    ]);

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-unicode");
    }

    #[test]
    fn exposes_stable_mode_labels_and_order() {
        let labels = UnicodeMode::ALL.map(UnicodeMode::label);

        assert_eq!(
            labels,
            [
                "grapheme",
                "bidi",
                "zero-width",
                "emoji",
                "normalization",
                "mixed",
                "invalid-utf8",
            ]
        );
    }

    #[test]
    fn parses_stable_mode_labels() {
        for mode in UnicodeMode::ALL {
            let parsed = mode
                .label()
                .parse::<UnicodeMode>()
                .expect("stable mode label should parse");

            assert_eq!(parsed, mode);
            assert_eq!(parsed.to_string(), mode.label());
        }
    }

    #[test]
    fn rejects_unknown_mode_label_with_invalid_argument() {
        let error = "confusables"
            .parse::<UnicodeMode>()
            .expect_err("unknown mode should fail");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("unsupported Unicode mode"));
        assert!(error.to_string().contains("invalid-utf8"));
    }

    #[test]
    fn exposes_stable_output_kind_labels_and_order() {
        let labels = UnicodeOutputKind::ALL.map(UnicodeOutputKind::label);

        assert_eq!(labels, ["valid-text", "raw-bytes"]);
    }

    #[test]
    fn parses_stable_output_kind_labels() {
        for output_kind in UnicodeOutputKind::ALL {
            let parsed = output_kind
                .label()
                .parse::<UnicodeOutputKind>()
                .expect("stable output kind label should parse");

            assert_eq!(parsed, output_kind);
            assert_eq!(parsed.to_string(), output_kind.label());
        }
    }

    #[test]
    fn rejects_invalid_utf8_mode_for_valid_text_output() {
        let error = validate_mode_output(UnicodeMode::InvalidUtf8, UnicodeOutputKind::ValidText)
            .expect_err("invalid UTF-8 must not be valid text");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("invalid-utf8"));
        assert!(error.to_string().contains("valid-text"));
        assert!(error.to_string().contains("raw-bytes"));
    }

    #[test]
    fn accepts_invalid_utf8_mode_for_raw_byte_output() {
        let spec = UnicodeFixtureSpec::new(UnicodeMode::InvalidUtf8, UnicodeOutputKind::RawBytes)
            .expect("invalid UTF-8 is supported as raw bytes");

        assert_eq!(spec.mode(), UnicodeMode::InvalidUtf8);
        assert_eq!(spec.output_kind(), UnicodeOutputKind::RawBytes);
    }

    #[test]
    fn accepts_all_non_invalid_modes_for_valid_text_output() {
        for mode in UnicodeMode::ALL {
            if mode == UnicodeMode::InvalidUtf8 {
                continue;
            }

            UnicodeFixtureSpec::new(mode, UnicodeOutputKind::ValidText)
                .expect("valid Unicode modes should support valid text output");
        }
    }

    #[test]
    fn valid_text_generation_is_deterministic_for_same_seed_mode_and_count() {
        let left = generate_valid_text(&TEST_SEED, UnicodeMode::Grapheme, 12)
            .expect("valid-text generation should succeed");
        let right = generate_valid_text(&TEST_SEED, UnicodeMode::Grapheme, 12)
            .expect("valid-text generation should succeed");

        assert_eq!(left, right);
    }

    #[test]
    fn raw_byte_generation_is_deterministic_for_same_seed_mode_and_count() {
        let left = generate_raw_bytes(&TEST_SEED, UnicodeMode::InvalidUtf8, 12)
            .expect("raw-byte generation should succeed");
        let right = generate_raw_bytes(&TEST_SEED, UnicodeMode::InvalidUtf8, 12)
            .expect("raw-byte generation should succeed");

        assert_eq!(left, right);
    }

    #[test]
    fn seed_1337_valid_text_grapheme_matches_golden() {
        let output = generate_valid_text(&seed_1337(), UnicodeMode::Grapheme, 12)
            .expect("grapheme valid-text generation should succeed");

        assert_eq!(
            bytes_to_hex(output.as_bytes()),
            fixture("seed_1337_unicode_valid_text_grapheme.hex")
        );
    }

    #[test]
    fn seed_1337_valid_text_mixed_matches_golden() {
        let output = generate_valid_text(&seed_1337(), UnicodeMode::Mixed, 12)
            .expect("mixed valid-text generation should succeed");

        assert_eq!(
            bytes_to_hex(output.as_bytes()),
            fixture("seed_1337_unicode_valid_text_mixed.hex")
        );
    }

    #[test]
    fn seed_1337_raw_bytes_invalid_utf8_matches_golden_hex() {
        let output = generate_raw_bytes(&seed_1337(), UnicodeMode::InvalidUtf8, 12)
            .expect("invalid-utf8 raw-byte generation should succeed");

        assert_eq!(
            bytes_to_hex(&output),
            fixture("seed_1337_unicode_raw_bytes_invalid_utf8.hex")
        );
    }

    #[test]
    fn seed_1337_raw_bytes_mixed_matches_golden_hex() {
        let output = generate_raw_bytes(&seed_1337(), UnicodeMode::Mixed, 12)
            .expect("mixed raw-byte generation should succeed");

        assert_eq!(
            bytes_to_hex(&output),
            fixture("seed_1337_unicode_raw_bytes_mixed.hex")
        );
    }

    #[test]
    fn valid_text_generation_uses_mode_specific_stream_context() {
        let grapheme = generate_valid_text(&TEST_SEED, UnicodeMode::Grapheme, 8)
            .expect("grapheme generation should succeed");
        let emoji = generate_valid_text(&TEST_SEED, UnicodeMode::Emoji, 8)
            .expect("emoji generation should succeed");

        assert_ne!(grapheme, emoji);
    }

    #[test]
    fn valid_text_generation_rejects_invalid_utf8_mode() {
        let error = generate_valid_text(&TEST_SEED, UnicodeMode::InvalidUtf8, 1)
            .expect_err("invalid-utf8 mode must not generate valid text");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("invalid-utf8"));
        assert!(error.to_string().contains("valid-text"));
    }

    #[test]
    fn valid_text_generation_returns_empty_string_for_zero_count() {
        let output = generate_valid_text(&TEST_SEED, UnicodeMode::Mixed, 0)
            .expect("zero valid-text cases should succeed");

        assert_eq!(output, "");
    }

    #[test]
    fn raw_byte_generation_returns_empty_vec_for_zero_count() {
        let output = generate_raw_bytes(&TEST_SEED, UnicodeMode::InvalidUtf8, 0)
            .expect("zero raw-byte cases should succeed");

        assert!(output.is_empty());
    }

    #[test]
    fn every_valid_text_mode_generates_non_empty_valid_utf8() {
        for mode in UnicodeMode::ALL {
            if mode == UnicodeMode::InvalidUtf8 {
                continue;
            }

            let output = generate_valid_text(&TEST_SEED, mode, 3)
                .expect("valid-text mode should generate valid UTF-8");

            assert!(!output.is_empty(), "{mode} output should not be empty");
            assert!(std::str::from_utf8(output.as_bytes()).is_ok());
        }
    }

    #[test]
    fn invalid_utf8_raw_byte_generation_returns_invalid_utf8_bytes() {
        let output = generate_raw_bytes(&TEST_SEED, UnicodeMode::InvalidUtf8, 8)
            .expect("invalid-utf8 raw-byte generation should succeed");

        assert!(!output.is_empty());
        assert!(std::str::from_utf8(&output).is_err());
    }

    #[test]
    fn valid_modes_raw_byte_generation_returns_valid_utf8_bytes() {
        for mode in UnicodeMode::ALL {
            if matches!(mode, UnicodeMode::InvalidUtf8 | UnicodeMode::Mixed) {
                continue;
            }

            let output = generate_raw_bytes(&TEST_SEED, mode, 8)
                .expect("valid raw-byte mode should generate bytes");

            assert!(!output.is_empty(), "{mode} output should not be empty");
            assert!(
                std::str::from_utf8(&output).is_ok(),
                "{mode} raw-byte output should remain valid UTF-8"
            );
        }
    }

    #[test]
    fn mixed_raw_byte_generation_includes_invalid_utf8_for_larger_sample() {
        let output = generate_raw_bytes(&TEST_SEED, UnicodeMode::Mixed, 64)
            .expect("mixed raw-byte generation should succeed");

        assert!(!output.is_empty());
        assert!(std::str::from_utf8(&output).is_err());
    }

    #[test]
    fn valid_text_fixture_families_cover_required_categories() {
        assert!(
            GRAPHEME_FIXTURES
                .iter()
                .any(|fixture| fixture.contains('\u{0301}')),
            "grapheme fixtures should include combining marks"
        );
        assert!(
            BIDI_FIXTURES
                .iter()
                .any(|fixture| fixture.contains('\u{202E}') || fixture.contains('\u{2067}')),
            "bidi fixtures should include directional controls"
        );
        assert!(
            ZERO_WIDTH_FIXTURES
                .iter()
                .any(|fixture| fixture.contains('\u{200B}') || fixture.contains('\u{200D}')),
            "zero-width fixtures should include invisible controls"
        );
        assert!(
            EMOJI_FIXTURES
                .iter()
                .any(|fixture| fixture.contains('\u{1F469}') || fixture.contains('\u{1F44D}')),
            "emoji fixtures should include emoji sequences"
        );
        assert!(
            NORMALIZATION_FIXTURES
                .iter()
                .any(|fixture| fixture.contains('\u{0301}') || fixture.contains('\u{212B}')),
            "normalization fixtures should include composed or compatibility variants"
        );
        assert_eq!(VALID_TEXT_FAMILIES.len(), 5);
    }

    #[test]
    fn mixed_valid_text_generation_samples_only_valid_fixture_families() {
        let output = generate_valid_text(&TEST_SEED, UnicodeMode::Mixed, 64)
            .expect("mixed valid-text generation should succeed");

        assert!(!output.is_empty());
        assert!(!output.contains('\u{FFFD}'));

        let sampled_family_count = VALID_TEXT_FAMILIES
            .iter()
            .filter(|fixtures| output.lines().any(|case| fixtures.contains(&case)))
            .count();

        assert!(
            sampled_family_count > 1,
            "mixed mode should sample across valid-text fixture families"
        );

        for case in output.lines() {
            assert!(
                VALID_TEXT_FAMILIES
                    .iter()
                    .any(|fixtures| fixtures.contains(&case)),
                "mixed case should come from a valid-text family: {case:?}"
            );
        }
    }

    fn seed_1337() -> MasterSeed {
        MasterSeed::from_str("1337").expect("integer seed 1337 should parse")
    }

    fn bytes_to_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }

    fn fixture(name: &str) -> &'static str {
        match name {
            "seed_1337_unicode_valid_text_grapheme.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_unicode_valid_text_grapheme.hex"
            ))
            .trim(),
            "seed_1337_unicode_valid_text_mixed.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_unicode_valid_text_mixed.hex"
            ))
            .trim(),
            "seed_1337_unicode_raw_bytes_invalid_utf8.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_unicode_raw_bytes_invalid_utf8.hex"
            ))
            .trim(),
            "seed_1337_unicode_raw_bytes_mixed.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_unicode_raw_bytes_mixed.hex"
            ))
            .trim(),
            _ => panic!("unknown golden fixture '{name}'"),
        }
    }
}
