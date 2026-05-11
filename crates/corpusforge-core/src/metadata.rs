// SPDX-License-Identifier: Apache-2.0

//! Reproducibility metadata shared by orchestration layers.

/// Plain run metadata used to identify reproducible corpus operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunMetadata {
    /// CorpusForge tool version that produced the run.
    pub tool_version: String,
    /// User-visible command associated with the run.
    pub command: String,
    /// Canonical lowercase hexadecimal seed text.
    pub seed_hex: String,
    /// Optional profile hash when a profile participates in the run.
    pub profile_hash: Option<String>,
    /// Generation or processing engine name.
    pub engine: String,
    /// Generation or processing engine version.
    pub engine_version: String,
    /// Stable hash of command flags that affect reproducibility.
    pub flags_hash: String,
}

impl RunMetadata {
    /// Builds run metadata from caller-supplied reproducibility fields.
    pub fn new(
        tool_version: impl Into<String>,
        command: impl Into<String>,
        seed_hex: impl Into<String>,
        profile_hash: Option<String>,
        engine: impl Into<String>,
        engine_version: impl Into<String>,
        flags_hash: impl Into<String>,
    ) -> Self {
        Self {
            tool_version: tool_version.into(),
            command: command.into(),
            seed_hex: seed_hex.into(),
            profile_hash,
            engine: engine.into(),
            engine_version: engine_version.into(),
            flags_hash: flags_hash.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RunMetadata;

    #[test]
    fn constructs_plain_run_metadata() {
        let metadata = RunMetadata::new(
            "0.1.0",
            "corpusforge gen --seed hex:00",
            "000102",
            Some("cff:abc123".to_string()),
            "unicode",
            "1",
            "flags:def456",
        );

        assert_eq!(metadata.tool_version, "0.1.0");
        assert_eq!(metadata.command, "corpusforge gen --seed hex:00");
        assert_eq!(metadata.seed_hex, "000102");
        assert_eq!(metadata.profile_hash, Some("cff:abc123".to_string()));
        assert_eq!(metadata.engine, "unicode");
        assert_eq!(metadata.engine_version, "1");
        assert_eq!(metadata.flags_hash, "flags:def456");
    }
}
