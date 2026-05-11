// SPDX-License-Identifier: Apache-2.0

//! Placeholder crate for the CorpusForge `.cff` profile format.

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-cff"
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-cff");
    }
}
