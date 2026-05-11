// SPDX-License-Identifier: Apache-2.0

//! Placeholder crate for CorpusForge failure-case shrinking.

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-shrink"
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-shrink");
    }
}
