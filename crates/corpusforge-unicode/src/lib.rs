// SPDX-License-Identifier: Apache-2.0

//! Placeholder crate for CorpusForge Unicode adversarial cases.

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-unicode"
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-unicode");
    }
}
