// SPDX-License-Identifier: Apache-2.0

//! Tokenizer case specifications, stdin harness execution, and stable reports.

use corpusforge_core::seed::MasterSeed;
use corpusforge_core::Result;
use corpusforge_unicode::{generate_raw_bytes, generate_valid_text, UnicodeFixtureSpec};
pub use corpusforge_unicode::{UnicodeMode, UnicodeOutputKind};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Tokenizer case request backed by deterministic Unicode generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenizerCaseSpec {
    mode: UnicodeMode,
    output_kind: UnicodeOutputKind,
    case_count: usize,
}

impl TokenizerCaseSpec {
    /// Builds a validated tokenizer case specification.
    pub fn new(
        mode: UnicodeMode,
        output_kind: UnicodeOutputKind,
        case_count: usize,
    ) -> Result<Self> {
        UnicodeFixtureSpec::new(mode, output_kind)?;

        Ok(Self {
            mode,
            output_kind,
            case_count,
        })
    }

    /// Returns the Unicode mode used for generation.
    pub const fn mode(self) -> UnicodeMode {
        self.mode
    }

    /// Returns the output boundary used for generation.
    pub const fn output_kind(self) -> UnicodeOutputKind {
        self.output_kind
    }

    /// Returns the number of generated cases requested.
    pub const fn case_count(self) -> usize {
        self.case_count
    }
}

/// One generated tokenizer sample with a stable zero-based case index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizerCase {
    case_index: usize,
    bytes: Vec<u8>,
}

impl TokenizerCase {
    fn new(case_index: usize, bytes: Vec<u8>) -> Self {
        Self { case_index, bytes }
    }

    /// Returns the stable zero-based case index.
    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    /// Returns the generated sample bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the generated sample byte count.
    pub fn byte_count(&self) -> usize {
        self.bytes.len()
    }
}

/// Generates deterministic tokenizer cases from a validated Unicode-backed spec.
pub fn generate_tokenizer_cases(
    master_seed: &MasterSeed,
    spec: TokenizerCaseSpec,
) -> Result<Vec<TokenizerCase>> {
    let bytes = match spec.output_kind {
        UnicodeOutputKind::ValidText => {
            generate_valid_text(master_seed, spec.mode, spec.case_count)?.into_bytes()
        }
        UnicodeOutputKind::RawBytes => generate_raw_bytes(master_seed, spec.mode, spec.case_count)?,
    };

    Ok(split_generated_cases(bytes, spec.case_count))
}

fn split_generated_cases(bytes: Vec<u8>, case_count: usize) -> Vec<TokenizerCase> {
    if case_count == 0 {
        return Vec::new();
    }

    bytes
        .split(|byte| *byte == b'\n')
        .take(case_count)
        .enumerate()
        .map(|(case_index, sample)| TokenizerCase::new(case_index, sample.to_vec()))
        .collect()
}

/// Executable path and argv used to run a tokenizer harness without a shell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessCommand {
    executable: PathBuf,
    argv: Vec<String>,
}

impl HarnessCommand {
    /// Creates a harness command from an executable path and literal argv.
    pub fn new(executable: impl Into<PathBuf>, argv: impl IntoIterator<Item = String>) -> Self {
        Self {
            executable: executable.into(),
            argv: argv.into_iter().collect(),
        }
    }

    /// Returns the executable path.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the literal argv passed to the executable.
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    fn to_report_string(&self) -> String {
        let mut command = self.executable.display().to_string();
        for arg in &self.argv {
            command.push(' ');
            command.push_str(arg);
        }
        command
    }
}

/// Stable harness status used in reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HarnessStatus {
    /// Every sample completed successfully.
    Passed,
    /// At least one sample failed to start, write stdin, or exit successfully.
    Failed,
}

impl HarnessStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

/// First failing sample observed during harness execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureSample {
    case_index: usize,
    byte_count: usize,
    hex_bytes: String,
    exit_code: Option<i32>,
}

impl FailureSample {
    fn from_case(case: &TokenizerCase, exit_code: Option<i32>) -> Self {
        Self {
            case_index: case.case_index(),
            byte_count: case.byte_count(),
            hex_bytes: bytes_to_hex(case.bytes()),
            exit_code,
        }
    }

    /// Returns the failed case index.
    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    /// Returns the failed sample byte count.
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }

    /// Returns lowercase hexadecimal sample bytes.
    pub fn hex_bytes(&self) -> &str {
        &self.hex_bytes
    }

    /// Returns the process exit code when the process reached exit.
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}

/// Result of running a harness across generated tokenizer samples.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessRun {
    status: HarnessStatus,
    failure_sample: Option<FailureSample>,
}

impl HarnessRun {
    /// Returns the harness status.
    pub const fn status(&self) -> HarnessStatus {
        self.status
    }

    /// Returns the first failing sample, when any.
    pub const fn failure_sample(&self) -> Option<&FailureSample> {
        self.failure_sample.as_ref()
    }
}

/// Runs the executable once per sample, writing each sample to stdin.
pub fn run_stdin_harness(command: &HarnessCommand, cases: &[TokenizerCase]) -> HarnessRun {
    for case in cases {
        let status = run_one_sample(command, case);
        match status {
            Ok(Some(0)) => {}
            Ok(exit_code) => {
                return HarnessRun {
                    status: HarnessStatus::Failed,
                    failure_sample: Some(FailureSample::from_case(case, exit_code)),
                };
            }
            Err(()) => {
                return HarnessRun {
                    status: HarnessStatus::Failed,
                    failure_sample: Some(FailureSample::from_case(case, None)),
                };
            }
        }
    }

    HarnessRun {
        status: HarnessStatus::Passed,
        failure_sample: None,
    }
}

fn run_one_sample(
    command: &HarnessCommand,
    case: &TokenizerCase,
) -> std::result::Result<Option<i32>, ()> {
    let mut child = Command::new(command.executable())
        .args(command.argv())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;

    let mut stdin = child.stdin.take().ok_or(())?;
    if let Err(error) = stdin.write_all(case.bytes()) {
        if error.kind() != ErrorKind::BrokenPipe {
            return Err(());
        }
    }
    drop(stdin);

    let status = child.wait().map_err(|_| ())?;
    Ok(status.code())
}

/// Stable tokenizer report metadata and first-failure summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizerReport {
    tool_version: String,
    command: String,
    seed: String,
    profile_hash: Option<String>,
    unicode_mode: UnicodeMode,
    output_kind: UnicodeOutputKind,
    case_count: usize,
    harness_command: String,
    status: HarnessStatus,
    failure_sample: Option<FailureSample>,
}

impl TokenizerReport {
    /// Builds a stable report from generation metadata and a harness run.
    pub fn new(
        tool_version: impl Into<String>,
        command: impl Into<String>,
        seed: &MasterSeed,
        profile_hash: Option<String>,
        spec: TokenizerCaseSpec,
        harness_command: &HarnessCommand,
        run: HarnessRun,
    ) -> Self {
        Self {
            tool_version: tool_version.into(),
            command: command.into(),
            seed: seed.to_string(),
            profile_hash,
            unicode_mode: spec.mode(),
            output_kind: spec.output_kind(),
            case_count: spec.case_count(),
            harness_command: harness_command.to_report_string(),
            status: run.status(),
            failure_sample: run.failure_sample().cloned(),
        }
    }

    /// Formats the report as stable, hand-written JSON with deterministic field order.
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push('{');
        push_json_string_field(&mut json, "tool_version", &self.tool_version);
        json.push(',');
        push_json_string_field(&mut json, "command", &self.command);
        json.push(',');
        push_json_string_field(&mut json, "seed", &self.seed);
        json.push_str(",\"profile_hash\":");
        match &self.profile_hash {
            Some(profile_hash) => push_json_string(&mut json, profile_hash),
            None => json.push_str("null"),
        }
        json.push(',');
        push_json_string_field(&mut json, "unicode_mode", self.unicode_mode.label());
        json.push(',');
        push_json_string_field(&mut json, "output_kind", self.output_kind.label());
        json.push_str(",\"case_count\":");
        json.push_str(&self.case_count.to_string());
        json.push(',');
        push_json_string_field(&mut json, "harness_command", &self.harness_command);
        json.push(',');
        push_json_string_field(&mut json, "status", self.status.label());
        json.push_str(",\"failure_sample\":");
        match &self.failure_sample {
            Some(sample) => push_failure_sample(&mut json, sample),
            None => json.push_str("null"),
        }
        json.push('}');
        json
    }
}

fn push_failure_sample(json: &mut String, sample: &FailureSample) {
    json.push('{');
    json.push_str("\"case_index\":");
    json.push_str(&sample.case_index.to_string());
    json.push_str(",\"byte_count\":");
    json.push_str(&sample.byte_count.to_string());
    json.push(',');
    push_json_string_field(json, "hex_bytes", &sample.hex_bytes);
    json.push_str(",\"exit_code\":");
    match sample.exit_code {
        Some(exit_code) => json.push_str(&exit_code.to_string()),
        None => json.push_str("null"),
    }
    json.push('}');
}

fn push_json_string_field(json: &mut String, name: &str, value: &str) {
    push_json_string(json, name);
    json.push(':');
    push_json_string(json, value);
}

fn push_json_string(json: &mut String, value: &str) {
    json.push('"');
    for character in value.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\u{08}' => json.push_str("\\b"),
            '\u{0c}' => json.push_str("\\f"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            '\u{00}'..='\u{1f}' => {
                json.push_str("\\u");
                json.push_str(&format!("{:04x}", character as u32));
            }
            _ => json.push(character),
        }
    }
    json.push('"');
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-tokenizer"
}

#[cfg(test)]
mod tests {
    use super::{
        crate_name, generate_tokenizer_cases, run_stdin_harness, HarnessCommand, HarnessRun,
        HarnessStatus, TokenizerCase, TokenizerCaseSpec, TokenizerReport, UnicodeMode,
        UnicodeOutputKind,
    };
    use corpusforge_core::seed::MasterSeed;
    use std::io::{self, Read};
    use std::str::FromStr;

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-tokenizer");
    }

    #[test]
    fn deterministic_case_generation_preserves_indices_and_bytes() {
        let spec = TokenizerCaseSpec::new(UnicodeMode::Grapheme, UnicodeOutputKind::ValidText, 4)
            .expect("valid spec should be accepted");
        let left = generate_tokenizer_cases(&seed_1337(), spec).expect("generation should succeed");
        let right =
            generate_tokenizer_cases(&seed_1337(), spec).expect("generation should be repeatable");

        assert_eq!(left, right);
        assert_eq!(left.len(), 4);
        assert_eq!(left[0].case_index(), 0);
        assert_eq!(left[3].case_index(), 3);
        assert!(left.iter().all(|case| !case.bytes().is_empty()));
    }

    #[test]
    fn invalid_mode_output_combination_is_rejected() {
        let error =
            TokenizerCaseSpec::new(UnicodeMode::InvalidUtf8, UnicodeOutputKind::ValidText, 1)
                .expect_err("invalid UTF-8 cannot be valid text");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("invalid-utf8"));
        assert!(error.to_string().contains("valid-text"));
    }

    #[test]
    fn stdin_harness_passes_when_helper_exits_zero_for_each_sample() {
        let command = test_harness_command("tests::stdin_helper_accepts_nonempty_input");
        let cases = vec![
            TokenizerCase::new(0, b"alpha".to_vec()),
            TokenizerCase::new(1, b"beta".to_vec()),
        ];

        let run = run_stdin_harness(&command, &cases);

        assert_eq!(run.status(), HarnessStatus::Passed);
        assert!(run.failure_sample().is_none());
    }

    #[test]
    fn stdin_harness_passes_when_helper_exits_zero_without_reading_stdin() {
        let command = test_harness_command("tests::stdin_helper_exits_zero_without_reading_stdin");
        let cases = vec![TokenizerCase::new(0, vec![b'x'; 1_048_576])];

        let run = run_stdin_harness(&command, &cases);

        assert_eq!(run.status(), HarnessStatus::Passed);
        assert!(run.failure_sample().is_none());
    }

    #[test]
    fn stdin_harness_reports_first_nonzero_exit() {
        let command = test_harness_command("tests::stdin_helper_rejects_fail_input");
        let cases = vec![
            TokenizerCase::new(0, b"ok".to_vec()),
            TokenizerCase::new(1, b"fail".to_vec()),
            TokenizerCase::new(2, b"not-run".to_vec()),
        ];

        let run = run_stdin_harness(&command, &cases);
        let failure = run
            .failure_sample()
            .expect("non-zero helper exit should record failure");

        assert_eq!(run.status(), HarnessStatus::Failed);
        assert_eq!(failure.case_index(), 1);
        assert_eq!(failure.byte_count(), 4);
        assert_eq!(failure.hex_bytes(), "6661696c");
        assert_eq!(failure.exit_code(), Some(101));
    }

    #[test]
    fn stable_json_fields_are_ordered_and_profile_hash_can_be_null() {
        let spec = TokenizerCaseSpec::new(UnicodeMode::InvalidUtf8, UnicodeOutputKind::RawBytes, 1)
            .expect("raw invalid UTF-8 should be accepted");
        let command = HarnessCommand::new("tokenizer-harness", ["--strict".to_string()]);
        let run = HarnessRun {
            status: HarnessStatus::Failed,
            failure_sample: Some(super::FailureSample::from_case(
                &TokenizerCase::new(0, vec![0x80]),
                Some(2),
            )),
        };
        let report = TokenizerReport::new(
            "0.1.0",
            "tokenizer",
            &seed_1337(),
            None,
            spec,
            &command,
            run,
        );

        assert_eq!(
            report.to_json(),
            concat!(
                "{\"tool_version\":\"0.1.0\",",
                "\"command\":\"tokenizer\",",
                "\"seed\":\"",
                "096875ea372a1b80bcccb9d8f3f10dde3e0f65e6facc94bf477e3b9531c7aa51",
                "\",\"profile_hash\":null,",
                "\"unicode_mode\":\"invalid-utf8\",",
                "\"output_kind\":\"raw-bytes\",",
                "\"case_count\":1,",
                "\"harness_command\":\"tokenizer-harness --strict\",",
                "\"status\":\"failed\",",
                "\"failure_sample\":{",
                "\"case_index\":0,",
                "\"byte_count\":1,",
                "\"hex_bytes\":\"80\",",
                "\"exit_code\":2",
                "}}"
            )
        );
    }

    #[test]
    fn json_escaping_is_stable_for_report_strings() {
        let spec = TokenizerCaseSpec::new(UnicodeMode::Emoji, UnicodeOutputKind::ValidText, 0)
            .expect("valid spec should be accepted");
        let command = HarnessCommand::new(
            "tok\\bin",
            ["quote\"arg".to_string(), "line\narg".to_string()],
        );
        let report = TokenizerReport::new(
            "0.1\t0",
            "tokenizer \"check\"\nrun",
            &seed_1337(),
            Some("hash\\value".to_string()),
            spec,
            &command,
            HarnessRun {
                status: HarnessStatus::Passed,
                failure_sample: None,
            },
        );

        let json = report.to_json();

        assert!(json.contains("\"tool_version\":\"0.1\\t0\""));
        assert!(json.contains("\"command\":\"tokenizer \\\"check\\\"\\nrun\""));
        assert!(json.contains("\"profile_hash\":\"hash\\\\value\""));
        assert!(json.contains("\"harness_command\":\"tok\\\\bin quote\\\"arg line\\narg\""));
    }

    #[test]
    #[ignore]
    fn stdin_helper_accepts_nonempty_input() {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .expect("helper should read stdin");

        assert!(!input.is_empty());
    }

    #[test]
    #[ignore]
    fn stdin_helper_exits_zero_without_reading_stdin() {}

    #[test]
    #[ignore]
    fn stdin_helper_rejects_fail_input() {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .expect("helper should read stdin");

        assert_ne!(input, "fail");
    }

    fn test_harness_command(test_name: &str) -> HarnessCommand {
        let executable = std::env::current_exe().expect("current test executable should exist");
        HarnessCommand::new(
            executable,
            [
                "--ignored".to_string(),
                "--exact".to_string(),
                test_name.to_string(),
            ],
        )
    }

    fn seed_1337() -> MasterSeed {
        MasterSeed::from_str("1337").expect("integer seed 1337 should parse")
    }
}
