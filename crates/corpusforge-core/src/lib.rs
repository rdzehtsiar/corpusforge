// SPDX-License-Identifier: Apache-2.0

//! Shared core types for CorpusForge.

pub mod metadata;
pub mod output;
pub mod rng;
pub mod seed;
pub mod weighted;

#[cfg(test)]
mod golden;

use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Project-wide result type used by CorpusForge crates.
pub type Result<T> = std::result::Result<T, CorpusForgeError>;

/// Formats stable display labels for diagnostics and error messages.
pub fn stable_labels<T: Copy + Display, const N: usize>(items: &[T; N]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Project-wide error categories with actionable diagnostics.
#[derive(Debug)]
pub enum CorpusForgeError {
    /// Filesystem or stream I/O failed.
    Io(std::io::Error),
    /// A seed value could not be parsed or accepted.
    InvalidSeed { message: String },
    /// A caller supplied an invalid argument to a core API.
    InvalidArgument { message: String },
    /// A profile is malformed or violates profile rules.
    InvalidProfile { message: String },
    /// A profile, report, or data format version is unsupported.
    UnsupportedVersion { message: String },
    /// A deterministic operation produced inconsistent observable behavior.
    DeterminismViolation { message: String },
    /// A shrink or test predicate failed unexpectedly.
    PredicateFailure { message: String },
    /// A planned feature is intentionally not implemented yet.
    NotImplemented { feature: String },
}

impl CorpusForgeError {
    /// Returns a stable category label for diagnostics and tests.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::InvalidSeed { .. } => "invalid_seed",
            Self::InvalidArgument { .. } => "invalid_argument",
            Self::InvalidProfile { .. } => "invalid_profile",
            Self::UnsupportedVersion { .. } => "unsupported_version",
            Self::DeterminismViolation { .. } => "determinism_violation",
            Self::PredicateFailure { .. } => "predicate_failure",
            Self::NotImplemented { .. } => "not_implemented",
        }
    }

    /// Builds an invalid seed error.
    pub fn invalid_seed(message: impl Into<String>) -> Self {
        Self::InvalidSeed {
            message: message.into(),
        }
    }

    /// Builds an invalid argument error.
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            message: message.into(),
        }
    }

    /// Builds an invalid profile error.
    pub fn invalid_profile(message: impl Into<String>) -> Self {
        Self::InvalidProfile {
            message: message.into(),
        }
    }

    /// Builds an unsupported version error.
    pub fn unsupported_version(message: impl Into<String>) -> Self {
        Self::UnsupportedVersion {
            message: message.into(),
        }
    }

    /// Builds a determinism violation error.
    pub fn determinism_violation(message: impl Into<String>) -> Self {
        Self::DeterminismViolation {
            message: message.into(),
        }
    }

    /// Builds a predicate failure error.
    pub fn predicate_failure(message: impl Into<String>) -> Self {
        Self::PredicateFailure {
            message: message.into(),
        }
    }

    /// Builds a not implemented error for a planned feature.
    pub fn not_implemented(feature: impl Into<String>) -> Self {
        Self::NotImplemented {
            feature: feature.into(),
        }
    }
}

impl Display for CorpusForgeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::InvalidSeed { message } => write!(formatter, "invalid seed: {message}"),
            Self::InvalidArgument { message } => write!(formatter, "invalid argument: {message}"),
            Self::InvalidProfile { message } => write!(formatter, "invalid profile: {message}"),
            Self::UnsupportedVersion { message } => {
                write!(formatter, "unsupported version: {message}")
            }
            Self::DeterminismViolation { message } => {
                write!(formatter, "determinism violation: {message}")
            }
            Self::PredicateFailure { message } => write!(formatter, "predicate failure: {message}"),
            Self::NotImplemented { feature } => write!(formatter, "not implemented: {feature}"),
        }
    }
}

impl Error for CorpusForgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSeed { .. }
            | Self::InvalidArgument { .. }
            | Self::InvalidProfile { .. }
            | Self::UnsupportedVersion { .. }
            | Self::DeterminismViolation { .. }
            | Self::PredicateFailure { .. }
            | Self::NotImplemented { .. } => None,
        }
    }
}

impl From<std::io::Error> for CorpusForgeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{stable_labels, CorpusForgeError, Result};
    use std::error::Error;
    use std::io;

    #[test]
    fn exposes_stable_categories() {
        let cases = [
            (
                CorpusForgeError::invalid_seed("seed is empty"),
                "invalid_seed",
            ),
            (
                CorpusForgeError::invalid_argument("bound is zero"),
                "invalid_argument",
            ),
            (
                CorpusForgeError::invalid_profile("missing version"),
                "invalid_profile",
            ),
            (
                CorpusForgeError::unsupported_version("cff version 99"),
                "unsupported_version",
            ),
            (
                CorpusForgeError::determinism_violation("stream diverged"),
                "determinism_violation",
            ),
            (
                CorpusForgeError::predicate_failure("exit status 2"),
                "predicate_failure",
            ),
            (
                CorpusForgeError::not_implemented("profile compile"),
                "not_implemented",
            ),
        ];

        for (error, category) in cases {
            assert_eq!(error.category(), category);
        }
    }

    #[test]
    fn display_includes_category_context_and_detail() {
        let error = CorpusForgeError::invalid_seed("expected unsigned integer");

        assert_eq!(error.to_string(), "invalid seed: expected unsigned integer");
    }

    #[test]
    fn io_conversion_preserves_source_and_category() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "profile.cff");
        let error = CorpusForgeError::from(io_error);

        assert_eq!(error.category(), "io");
        assert_eq!(error.to_string(), "I/O error: profile.cff");
        assert!(error.source().is_some());
    }

    #[test]
    fn result_alias_uses_project_error() {
        fn fail() -> Result<()> {
            Err(CorpusForgeError::not_implemented("generation"))
        }

        let error = fail().expect_err("result should use CorpusForgeError");
        assert_eq!(error.category(), "not_implemented");
    }

    #[test]
    fn stable_labels_joins_display_values_in_input_order() {
        assert_eq!(
            stable_labels(&["alpha", "beta", "gamma"]),
            "alpha, beta, gamma"
        );
    }
}
