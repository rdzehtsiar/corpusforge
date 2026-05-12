// SPDX-License-Identifier: Apache-2.0

//! Deterministic byte-level failure-case shrinking for CorpusForge.

use corpusforge_core::{CorpusForgeError, Result};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Default predicate timeout in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 1_000;

/// Default maximum predicate executions for one shrink operation.
pub const DEFAULT_MAX_RUNS: usize = 10_000;

/// Executable path and literal argv used to run a shrink predicate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredicateCommand {
    executable: PathBuf,
    argv: Vec<String>,
}

impl PredicateCommand {
    /// Creates a predicate command from an executable path and literal argv.
    pub fn new<I, S>(executable: impl Into<PathBuf>, argv: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            executable: executable.into(),
            argv: argv.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the executable path invoked directly without a shell.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the literal argv passed to the executable in order.
    pub fn argv(&self) -> &[String] {
        &self.argv
    }
}

/// Configuration for deterministic byte shrinking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShrinkConfig {
    predicate: PredicateCommand,
    timeout_ms: u64,
    max_runs: usize,
}

impl ShrinkConfig {
    /// Builds a shrink configuration with default timeout and run limit.
    pub fn new(predicate: PredicateCommand) -> Self {
        Self {
            predicate,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_runs: DEFAULT_MAX_RUNS,
        }
    }

    /// Sets the per-run timeout in milliseconds.
    pub const fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Sets the maximum number of predicate executions.
    pub const fn with_max_runs(mut self, max_runs: usize) -> Self {
        self.max_runs = max_runs;
        self
    }

    /// Returns the predicate command.
    pub const fn predicate(&self) -> &PredicateCommand {
        &self.predicate
    }

    /// Returns the per-run timeout in milliseconds.
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Returns the maximum number of predicate executions.
    pub const fn max_runs(&self) -> usize {
        self.max_runs
    }
}

/// Stable failure kind preserved by the shrinker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateFailureKind {
    /// The predicate exited with this non-zero status code.
    ExitCode(i32),
    /// The predicate exceeded the configured timeout.
    Timeout,
}

/// Result metadata returned by a shrink operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShrinkOutcome {
    reduced_bytes: Vec<u8>,
    original_byte_count: usize,
    reduced_byte_count: usize,
    predicate_runs: usize,
    original_hash: String,
    reduced_hash: String,
    failure_kind: PredicateFailureKind,
}

impl ShrinkOutcome {
    /// Returns the minimized byte sequence.
    pub fn reduced_bytes(&self) -> &[u8] {
        &self.reduced_bytes
    }

    /// Returns the original input byte count.
    pub const fn original_byte_count(&self) -> usize {
        self.original_byte_count
    }

    /// Returns the reduced input byte count.
    pub const fn reduced_byte_count(&self) -> usize {
        self.reduced_byte_count
    }

    /// Returns the number of predicate executions used.
    pub const fn predicate_runs(&self) -> usize {
        self.predicate_runs
    }

    /// Returns a stable FNV-1a hash of the original bytes.
    pub fn original_hash(&self) -> &str {
        &self.original_hash
    }

    /// Returns a stable FNV-1a hash of the reduced bytes.
    pub fn reduced_hash(&self) -> &str {
        &self.reduced_hash
    }

    /// Returns the preserved baseline failure kind.
    pub const fn failure_kind(&self) -> PredicateFailureKind {
        self.failure_kind
    }
}

/// Shrinks a failing byte input while preserving the original failure signature.
pub fn shrink_bytes(input: &[u8], config: &ShrinkConfig) -> Result<ShrinkOutcome> {
    validate_config(config)?;

    let mut budget = RunBudget::new(config.max_runs);
    let baseline = verify_original_failure(input, config, &mut budget)?;
    let mut best = input.to_vec();
    let mut granularity = 2usize;

    while !best.is_empty() && budget.remaining() >= 2 {
        let chunk_count = granularity.min(best.len());
        let chunk_size = best.len().div_ceil(chunk_count);
        let mut start = 0usize;
        let mut reduced_this_pass = false;

        while start < best.len() && budget.remaining() >= 2 {
            let end = (start + chunk_size).min(best.len());
            let candidate = without_range(&best, start, end);

            if preserves_failure(&candidate, config, &mut budget, baseline)? {
                best = candidate;
                granularity = 2;
                reduced_this_pass = true;
                break;
            }

            start = end;
        }

        if !reduced_this_pass {
            if granularity >= best.len() {
                break;
            }
            granularity = (granularity * 2).min(best.len());
        }
    }

    Ok(ShrinkOutcome {
        original_byte_count: input.len(),
        reduced_byte_count: best.len(),
        predicate_runs: budget.runs(),
        original_hash: stable_hash(input),
        reduced_hash: stable_hash(&best),
        failure_kind: baseline,
        reduced_bytes: best,
    })
}

fn validate_config(config: &ShrinkConfig) -> Result<()> {
    if config.predicate.executable.as_os_str().is_empty() {
        return Err(CorpusForgeError::invalid_argument(
            "shrink predicate executable must not be empty",
        ));
    }
    if config.timeout_ms == 0 {
        return Err(CorpusForgeError::invalid_argument(
            "shrink predicate timeout_ms must be greater than zero",
        ));
    }
    if config.max_runs < 2 {
        return Err(CorpusForgeError::invalid_argument(
            "shrink predicate max_runs must be at least 2 to confirm the original failure",
        ));
    }
    Ok(())
}

fn verify_original_failure(
    input: &[u8],
    config: &ShrinkConfig,
    budget: &mut RunBudget,
) -> Result<PredicateFailureKind> {
    let first = run_predicate(input, config, budget)?;
    let second = run_predicate(input, config, budget)?;

    match (first, second) {
        (PredicateRunStatus::Passed, PredicateRunStatus::Passed) => {
            Err(CorpusForgeError::invalid_argument(
                "original input passed the shrink predicate twice; shrinking requires a reproducible failure",
            ))
        }
        (PredicateRunStatus::Failed(left), PredicateRunStatus::Failed(right)) if left == right => {
            Ok(left)
        }
        (left, right) => Err(CorpusForgeError::determinism_violation(format!(
            "original input is flaky: first run {}, second run {}",
            left.describe(),
            right.describe()
        ))),
    }
}

fn preserves_failure(
    candidate: &[u8],
    config: &ShrinkConfig,
    budget: &mut RunBudget,
    baseline: PredicateFailureKind,
) -> Result<bool> {
    let first = run_predicate(candidate, config, budget)?;
    let second = run_predicate(candidate, config, budget)?;

    match (first, second) {
        (PredicateRunStatus::Passed, PredicateRunStatus::Passed) => Ok(false),
        (PredicateRunStatus::Failed(left), PredicateRunStatus::Failed(right)) if left == right => {
            Ok(left == baseline)
        }
        (left, right) => Err(CorpusForgeError::determinism_violation(format!(
            "candidate input is flaky: first run {}, second run {}",
            left.describe(),
            right.describe()
        ))),
    }
}

fn without_range(bytes: &[u8], start: usize, end: usize) -> Vec<u8> {
    let mut candidate = Vec::with_capacity(bytes.len() - (end - start));
    candidate.extend_from_slice(&bytes[..start]);
    candidate.extend_from_slice(&bytes[end..]);
    candidate
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PredicateRunStatus {
    Passed,
    Failed(PredicateFailureKind),
}

impl PredicateRunStatus {
    fn describe(self) -> String {
        match self {
            Self::Passed => "passed".to_string(),
            Self::Failed(PredicateFailureKind::ExitCode(code)) => {
                format!("failed with exit code {code}")
            }
            Self::Failed(PredicateFailureKind::Timeout) => "timed out".to_string(),
        }
    }
}

fn run_predicate(
    bytes: &[u8],
    config: &ShrinkConfig,
    budget: &mut RunBudget,
) -> Result<PredicateRunStatus> {
    budget.claim_run()?;

    let mut child = Command::new(config.predicate.executable())
        .args(config.predicate.argv())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            CorpusForgeError::predicate_failure(format!(
                "failed to start shrink predicate `{}`: {error}",
                config.predicate.executable().display()
            ))
        })?;

    let stdin = child.stdin.take().ok_or_else(|| {
        CorpusForgeError::predicate_failure("failed to open shrink predicate stdin")
    })?;
    let input = bytes.to_vec();
    let writer = thread::spawn(move || write_predicate_stdin(stdin, input));
    let timeout = Duration::from_millis(config.timeout_ms);
    let start = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            finish_stdin_writer(writer)?;
            if status.success() {
                return Ok(PredicateRunStatus::Passed);
            }
            return match status.code() {
                Some(code) => Ok(PredicateRunStatus::Failed(PredicateFailureKind::ExitCode(
                    code,
                ))),
                None => Err(CorpusForgeError::predicate_failure(
                    "shrink predicate terminated without an exit code",
                )),
            };
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            child.kill()?;
            child.wait()?;
            finish_stdin_writer(writer)?;
            return Ok(PredicateRunStatus::Failed(PredicateFailureKind::Timeout));
        }

        thread::sleep(next_poll_sleep(timeout - elapsed));
    }
}

fn write_predicate_stdin(
    mut stdin: std::process::ChildStdin,
    input: Vec<u8>,
) -> std::io::Result<()> {
    match stdin.write_all(&input) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error),
    }
}

fn finish_stdin_writer(writer: thread::JoinHandle<std::io::Result<()>>) -> Result<()> {
    writer
        .join()
        .map_err(|_| CorpusForgeError::predicate_failure("shrink predicate stdin writer panicked"))?
        .map_err(|error| {
            CorpusForgeError::predicate_failure(format!(
                "failed to write shrink candidate to predicate stdin: {error}"
            ))
        })
}

fn next_poll_sleep(remaining: Duration) -> Duration {
    let poll = Duration::from_millis(5);
    if remaining < poll {
        remaining
    } else {
        poll
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RunBudget {
    runs: usize,
    max_runs: usize,
}

impl RunBudget {
    const fn new(max_runs: usize) -> Self {
        Self { runs: 0, max_runs }
    }

    const fn runs(self) -> usize {
        self.runs
    }

    const fn remaining(self) -> usize {
        self.max_runs - self.runs
    }

    fn claim_run(&mut self) -> Result<()> {
        if self.runs >= self.max_runs {
            return Err(CorpusForgeError::predicate_failure(format!(
                "shrink predicate run limit exceeded after {} runs (max_runs={})",
                self.runs, self.max_runs
            )));
        }

        self.runs += 1;
        Ok(())
    }
}

fn stable_hash(bytes: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }

    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{
        shrink_bytes, PredicateCommand, PredicateFailureKind, ShrinkConfig, DEFAULT_MAX_RUNS,
        DEFAULT_TIMEOUT_MS,
    };
    use std::fs;
    use std::io::{self, Read};
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn default_config_uses_planned_limits() {
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_fails_on_fail_substring",
        ));

        assert_eq!(config.timeout_ms(), DEFAULT_TIMEOUT_MS);
        assert_eq!(config.max_runs(), DEFAULT_MAX_RUNS);
    }

    #[test]
    fn successful_reduction_preserves_nonzero_failure() {
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_fails_on_fail_substring",
        ));

        let outcome =
            shrink_bytes(b"prefix fail suffix", &config).expect("failing input should shrink");

        assert_eq!(outcome.reduced_bytes(), b"fail");
        assert_eq!(outcome.original_byte_count(), 18);
        assert_eq!(outcome.reduced_byte_count(), 4);
        assert_eq!(outcome.failure_kind(), PredicateFailureKind::ExitCode(101));
        assert_eq!(outcome.original_hash(), "fnv1a64:5f4eb528b1bb7cac");
        assert_eq!(outcome.reduced_hash(), "fnv1a64:ef0166791679f92d");
        assert!(outcome.predicate_runs() >= 4);
    }

    #[test]
    fn unchanged_but_failing_input_succeeds_when_no_deletion_preserves_failure() {
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_requires_exact_abc",
        ));

        let outcome =
            shrink_bytes(b"abc", &config).expect("exact failing input should be accepted");

        assert_eq!(outcome.reduced_bytes(), b"abc");
        assert_eq!(outcome.original_byte_count(), 3);
        assert_eq!(outcome.reduced_byte_count(), 3);
        assert_eq!(outcome.failure_kind(), PredicateFailureKind::ExitCode(101));
    }

    #[test]
    fn original_passing_input_is_rejected() {
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_fails_on_fail_substring",
        ));

        let error = shrink_bytes(b"passing input", &config)
            .expect_err("passing original input should be rejected");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("original input passed"));
    }

    #[test]
    fn nonzero_exit_mismatch_is_not_accepted() {
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_changes_exit_code_for_shorter_input",
        ));

        let outcome =
            shrink_bytes(b"xfail", &config).expect("baseline nonzero failure should shrink");

        assert_eq!(outcome.reduced_bytes(), b"xfail");
        assert_eq!(outcome.failure_kind(), PredicateFailureKind::ExitCode(9));
    }

    #[test]
    fn timeout_preservation_works_for_timeout_baseline() {
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_always_times_out",
        ))
        .with_timeout_ms(20)
        .with_max_runs(12);

        let outcome =
            shrink_bytes(b"slow", &config).expect("timeout baseline should be shrinkable");

        assert_eq!(outcome.reduced_bytes(), b"");
        assert_eq!(outcome.failure_kind(), PredicateFailureKind::Timeout);
        assert_eq!(outcome.original_byte_count(), 4);
        assert_eq!(outcome.reduced_byte_count(), 0);
    }

    #[test]
    fn flaky_predicate_is_rejected() {
        let _ = fs::remove_file(flaky_state_path());
        let config = ShrinkConfig::new(test_predicate_command(
            "tests::predicate_helper_alternates_failure_signature",
        ));

        let error = shrink_bytes(b"flaky", &config).expect_err("flaky original should be rejected");

        assert_eq!(error.category(), "determinism_violation");
        assert!(error.to_string().contains("original input is flaky"));

        let _ = fs::remove_file(flaky_state_path());
    }

    #[test]
    #[ignore]
    fn predicate_helper_fails_on_fail_substring() {
        let input = read_stdin();

        assert!(
            !input.windows(b"fail".len()).any(|window| window == b"fail"),
            "input contains fail"
        );
    }

    #[test]
    #[ignore]
    fn predicate_helper_requires_exact_abc() {
        let input = read_stdin();

        assert_ne!(input, b"abc");
    }

    #[test]
    #[ignore]
    fn predicate_helper_changes_exit_code_for_shorter_input() {
        let input = read_stdin();

        if input == b"fail" {
            std::process::exit(7);
        }
        if input.windows(b"fail".len()).any(|window| window == b"fail") {
            std::process::exit(9);
        }
    }

    #[test]
    #[ignore]
    fn predicate_helper_always_times_out() {
        thread::sleep(Duration::from_millis(250));
    }

    #[test]
    #[ignore]
    fn predicate_helper_alternates_failure_signature() {
        let path = flaky_state_path();
        if path.exists() {
            let _ = fs::remove_file(path);
            std::process::exit(9);
        }

        fs::write(path, b"seen").expect("helper should write flaky marker");
        std::process::exit(7);
    }

    fn test_predicate_command(test_name: &str) -> PredicateCommand {
        let executable = std::env::current_exe().expect("current test executable should exist");
        PredicateCommand::new(executable, ["--ignored", "--exact", test_name, "--quiet"])
    }

    fn read_stdin() -> Vec<u8> {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .expect("helper should read stdin");
        input
    }

    fn flaky_state_path() -> PathBuf {
        std::env::current_exe()
            .expect("current test executable should exist")
            .with_extension("shrink-flaky-state")
    }
}
