// SPDX-License-Identifier: Apache-2.0

//! Placeholder crate for CorpusForge Unicode adversarial cases.

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
        crate_name, validate_mode_output, UnicodeFixtureSpec, UnicodeMode, UnicodeOutputKind,
    };

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
}
