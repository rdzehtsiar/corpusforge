// SPDX-License-Identifier: Apache-2.0

use corpusforge_cff::{ProfileFile, ProfilePack};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn corpusforge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_corpusforge"))
}

#[test]
fn binary_help_exits_successfully() {
    let output = corpusforge()
        .arg("--help")
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("COMMANDS"));
    assert!(stdout.contains("profile"));
    assert!(stdout.contains("gen"));
}

#[test]
fn binary_command_help_exits_successfully() {
    for command in ["profile", "gen", "shrink", "replay", "verify", "ci"] {
        let output = corpusforge()
            .args([command, "-h"])
            .output()
            .expect("binary should run");

        assert!(output.status.success(), "{command} help should succeed");
        let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
        assert!(stdout.contains(&format!("corpusforge {command}")));
        assert!(stdout.contains("--profile <path>"));
        if command == "profile" {
            assert!(stdout.contains("build"));
            assert!(stdout.contains("inspect"));
            assert!(stdout.contains("verify"));
            assert!(stdout.contains("--out <path>"));
        } else if command == "ci" {
            assert!(stdout.contains("corpusforge ci tokenizer"));
            assert!(stdout.contains("--unicode <mode>"));
            assert!(stdout.contains("--output-kind <kind>"));
            assert!(stdout.contains("--cases <N>"));
            assert!(stdout.contains("--command <path>"));
            assert!(stdout.contains("--arg <value>"));
            assert!(stdout.contains("--report-out <path>"));
            assert!(stdout.contains("TokenizerReport"));
        } else {
            assert!(stdout.contains("--seed <seed>"));
            assert!(stdout.contains("--seed-file <path>"));
            assert!(stdout.contains("--out <path>"));
            assert!(stdout.contains("--bytes <N>"));
            assert!(stdout.contains("--determinism <mode>"));
            assert!(stdout.contains("--metadata-out <path>"));
            assert!(stdout.contains("--quiet"));
            assert!(stdout.contains("--json"));
            if command == "gen" {
                assert!(stdout.contains("generated binary bytes"));
                assert!(stdout.contains("--unicode <mode>"));
                assert!(stdout.contains("--output-kind <kind>"));
                assert!(stdout.contains("--cases <N>"));
                assert!(stdout.contains("invalid-utf8"));
                assert!(!stdout.contains("Planned for a later milestone"));
            } else {
                assert!(stdout.contains("EXAMPLES"));
            }
        }
    }
}

#[test]
fn binary_command_help_with_common_flags_exits_successfully() {
    let output = corpusforge()
        .args(["gen", "--seed", "1337", "--bytes", "1MB", "--help"])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("corpusforge gen"));
    assert!(stdout.contains("--bytes <N>"));
    assert!(stdout.contains("--unicode <mode>"));
    assert!(stdout.contains("generated binary bytes"));
    assert!(!stdout.contains("Planned for a later milestone"));
}

#[test]
fn binary_placeholder_execution_exits_nonzero() {
    let output = corpusforge()
        .arg("shrink")
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("error: not implemented"));
    assert!(stderr.contains("shrink command execution"));
}

#[test]
fn binary_common_flags_parse_before_placeholder_execution() {
    let output = corpusforge()
        .args([
            "replay",
            "--seed",
            "42",
            "--profile",
            "profiles/smoke.cff",
            "--out",
            "out.txt",
            "--bytes",
            "64KB",
            "--determinism",
            "strict",
            "--metadata-out",
            "report.json",
            "--quiet",
            "--json",
        ])
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("error: not implemented"));
    assert!(stderr.contains("replay command execution"));
}

#[test]
fn binary_ci_tokenizer_writes_passing_report_and_preserves_arg_order() {
    let temp = TestDir::new("ci-tokenizer-pass");
    let report = temp.path().join("report.json");
    let harness = std::env::var_os("CARGO_BIN_EXE_corpusforge")
        .map(PathBuf::from)
        .expect("corpusforge binary path should be available");

    let output = corpusforge()
        .args([
            "ci",
            "tokenizer",
            "--unicode",
            "grapheme",
            "--output-kind",
            "valid-text",
            "--cases",
            "2",
            "--seed",
            "1337",
            "--command",
        ])
        .arg(&harness)
        .args([
            "--arg",
            "--version",
            "--arg",
            "--literal-second",
            "--report-out",
        ])
        .arg(&report)
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("tokenizer ci passed"));
    assert!(stdout.contains("case_count: 2"));

    let json = fs::read_to_string(&report).expect("report should exist");
    assert_eq!(
        json,
        format!(
            concat!(
                "{{\"tool_version\":\"{}\",",
                "\"command\":\"ci tokenizer\",",
                "\"seed\":\"096875ea372a1b80bcccb9d8f3f10dde3e0f65e6facc94bf477e3b9531c7aa51\",",
                "\"profile_hash\":null,",
                "\"unicode_mode\":\"grapheme\",",
                "\"output_kind\":\"valid-text\",",
                "\"case_count\":2,",
                "\"harness_command\":\"{} --version --literal-second\",",
                "\"status\":\"passed\",",
                "\"failure_sample\":null}}"
            ),
            env!("CARGO_PKG_VERSION"),
            json_escape_for_test(&harness.display().to_string())
        )
    );
}

#[test]
fn binary_ci_tokenizer_treats_help_arg_as_literal_harness_arg() {
    let temp = TestDir::new("ci-tokenizer-help-arg");
    let harness = std::env::var_os("CARGO_BIN_EXE_corpusforge")
        .map(PathBuf::from)
        .expect("corpusforge binary path should be available");

    for help_arg in ["--help", "-h"] {
        let report = temp.path().join(format!("report-{help_arg}.json"));
        let output = corpusforge()
            .args([
                "ci",
                "tokenizer",
                "--unicode",
                "grapheme",
                "--output-kind",
                "valid-text",
                "--cases",
                "1",
                "--seed",
                "1337",
                "--command",
            ])
            .arg(&harness)
            .args(["--arg", help_arg, "--report-out"])
            .arg(&report)
            .output()
            .expect("binary should run");

        assert!(
            output.status.success(),
            "--arg {help_arg} should be passed to the harness"
        );
        assert!(output.stderr.is_empty());

        let json = fs::read_to_string(&report).expect("report should exist");
        assert!(json.contains("\"status\":\"passed\""));
        assert!(json.contains(&format!(
            "\"harness_command\":\"{} {help_arg}\"",
            json_escape_for_test(&harness.display().to_string())
        )));
    }
}

#[test]
fn binary_ci_tokenizer_writes_failing_report_with_failure_sample() {
    let temp = TestDir::new("ci-tokenizer-fail");
    let report = temp.path().join("report.json");
    let harness = std::env::var_os("CARGO_BIN_EXE_corpusforge")
        .map(PathBuf::from)
        .expect("corpusforge binary path should be available");

    let output = corpusforge()
        .args([
            "ci",
            "tokenizer",
            "--unicode",
            "mixed",
            "--output-kind",
            "valid-text",
            "--cases",
            "2",
            "--seed",
            "1337",
            "--command",
        ])
        .arg(&harness)
        .args(["--arg", "unknown", "--report-out"])
        .arg(&report)
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("error: predicate failure"));

    let json = fs::read_to_string(&report).expect("report should exist");
    assert_eq!(
        json,
        format!(
            concat!(
                "{{\"tool_version\":\"{}\",",
                "\"command\":\"ci tokenizer\",",
                "\"seed\":\"096875ea372a1b80bcccb9d8f3f10dde3e0f65e6facc94bf477e3b9531c7aa51\",",
                "\"profile_hash\":null,",
                "\"unicode_mode\":\"mixed\",",
                "\"output_kind\":\"valid-text\",",
                "\"case_count\":2,",
                "\"harness_command\":\"{} unknown\",",
                "\"status\":\"failed\",",
                "\"failure_sample\":{{",
                "\"case_index\":0,",
                "\"byte_count\":13,",
                "\"hex_bytes\":\"e2808f72746c206d61726b6572\",",
                "\"exit_code\":1",
                "}}}}"
            ),
            env!("CARGO_PKG_VERSION"),
            json_escape_for_test(&harness.display().to_string())
        )
    );
}

#[test]
fn binary_ci_tokenizer_rejects_missing_required_args() {
    let cases: [(Vec<OsString>, &str); 3] = [
        (
            vec![
                "ci".into(),
                "tokenizer".into(),
                "--cases".into(),
                "1".into(),
            ],
            "missing required option `--unicode`",
        ),
        (
            vec![
                "ci".into(),
                "tokenizer".into(),
                "--unicode".into(),
                "mixed".into(),
                "--output-kind".into(),
                "valid-text".into(),
                "--cases".into(),
                "1".into(),
                "--seed".into(),
                "1337".into(),
            ],
            "missing required option `--command`",
        ),
        (
            vec![
                "ci".into(),
                "tokenizer".into(),
                "--unicode".into(),
                "mixed".into(),
                "--output-kind".into(),
                "valid-text".into(),
                "--cases".into(),
                "1".into(),
                "--seed".into(),
                "1337".into(),
                "--command".into(),
                "tokenizer-harness".into(),
            ],
            "missing required option `--report-out`",
        ),
    ];

    for (args, expected) in cases {
        assert_invalid_argument(args, expected, expected);
    }
}

#[test]
fn binary_profile_build_inspect_and_verify_succeed() {
    let temp = TestDir::new("profile-roundtrip");
    temp.write("input/zeta.txt", b"zeta");
    temp.write("input/nested/alpha.txt", b"alpha");
    let output_profile = temp.path().join("compiled.cff");

    let build = corpusforge()
        .args(["profile", "build"])
        .arg(temp.path().join("input"))
        .args(["--out"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");

    assert!(build.status.success());
    let stdout = String::from_utf8(build.stdout).expect("stdout should be UTF-8");
    assert_profile_summary(&stdout);
    assert!(stdout.contains("built profile"));
    assert!(output_profile.exists());

    let inspect = corpusforge()
        .args(["profile", "inspect", "--profile"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");

    assert!(inspect.status.success());
    let stdout = String::from_utf8(inspect.stdout).expect("stdout should be UTF-8");
    assert_profile_summary(&stdout);
    assert!(stdout.contains("inspected profile"));

    let verify = corpusforge()
        .args(["profile", "verify", "--profile"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");

    assert!(verify.status.success());
    let stdout = String::from_utf8(verify.stdout).expect("stdout should be UTF-8");
    assert_profile_summary(&stdout);
    assert!(stdout.contains("verified profile"));
}

#[test]
fn binary_top_level_verify_profile_alias_succeeds() {
    let temp = TestDir::new("verify-alias");
    let output_profile = temp.path().join("compiled.cff");

    let build = corpusforge()
        .args(["profile", "build"])
        .arg(repository_fixtures_path())
        .args(["--out"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");
    assert!(build.status.success());

    let verify = corpusforge()
        .args(["verify", "--profile"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");

    assert!(verify.status.success());
    let stdout = String::from_utf8(verify.stdout).expect("stdout should be UTF-8");
    assert_eq!(stdout, expected_fixture_summary("verified profile"));
}

#[test]
fn binary_profile_json_uses_stable_fields() {
    let temp = TestDir::new("profile-json");
    temp.write("fixture.txt", b"fixture");
    let output_profile = temp.path().join("compiled.cff");

    let build = corpusforge()
        .args(["profile", "build"])
        .arg(temp.path().join("fixture.txt"))
        .args(["--out"])
        .arg(&output_profile)
        .arg("--json")
        .output()
        .expect("binary should run");

    assert!(build.status.success());
    let stdout = String::from_utf8(build.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("\"version\":0"));
    assert!(stdout.contains("\"profile_hash\":\"cff:"));
    assert!(stdout.contains("\"file_count\":1"));
    assert!(stdout.contains("\"byte_count\":7"));
}

#[test]
fn binary_profile_inspect_output_matches_fixture_summary() {
    let temp = TestDir::new("fixture-inspect");
    let output_profile = temp.path().join("compiled.cff");

    let build = corpusforge()
        .args(["profile", "build"])
        .arg(repository_fixtures_path())
        .args(["--out"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");
    assert!(build.status.success());

    let inspect = corpusforge()
        .args(["profile", "inspect", "--profile"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");

    assert!(inspect.status.success());
    let stdout = String::from_utf8(inspect.stdout).expect("stdout should be UTF-8");
    assert_eq!(stdout, expected_fixture_summary("inspected profile"));
}

#[test]
fn binary_gen_stdout_emits_exact_deterministic_bytes() {
    let temp = TestDir::new("gen-stdout");
    let profile = build_repository_profile(&temp);

    let first = corpusforge()
        .args(["gen", "--profile"])
        .arg(&profile)
        .args(["--seed", "1337", "--bytes", "64"])
        .output()
        .expect("binary should run");
    assert!(first.status.success());
    assert_eq!(first.stdout.len(), 64);
    assert!(first.stderr.is_empty());

    let second = corpusforge()
        .args(["gen", "--profile"])
        .arg(&profile)
        .args(["--seed", "1337", "--bytes", "64"])
        .output()
        .expect("binary should run");
    assert!(second.status.success());
    assert_eq!(second.stdout, first.stdout);
    assert!(second.stderr.is_empty());
}

#[test]
fn binary_gen_repository_profile_seed_1337_matches_golden_hex() {
    let temp = TestDir::new("gen-golden");
    let profile = build_repository_profile(&temp);

    let output = corpusforge()
        .args(["gen", "--profile"])
        .arg(&profile)
        .args(["--seed", "1337", "--bytes", "64"])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(
        bytes_to_hex(&output.stdout),
        fixture("seed_1337_repository_fixtures_ngram.hex")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn binary_gen_unicode_valid_text_seed_1337_matches_golden_hex() {
    let first = corpusforge()
        .args([
            "gen",
            "--unicode",
            "mixed",
            "--output-kind",
            "valid-text",
            "--cases",
            "12",
            "--seed",
            "1337",
        ])
        .output()
        .expect("binary should run");

    assert!(first.status.success());
    assert!(first.stderr.is_empty());
    assert_eq!(
        bytes_to_hex(&first.stdout),
        fixture("seed_1337_unicode_valid_text_mixed.hex")
    );
    assert!(std::str::from_utf8(&first.stdout).is_ok());
    assert_ne!(first.stdout.last(), Some(&b'\n'));

    let second = corpusforge()
        .args([
            "gen",
            "--unicode",
            "mixed",
            "--output-kind",
            "valid-text",
            "--cases",
            "12",
            "--seed",
            "1337",
        ])
        .output()
        .expect("binary should run");

    assert!(second.status.success());
    assert_eq!(second.stdout, first.stdout);
}

#[test]
fn binary_gen_unicode_raw_bytes_invalid_utf8_seed_1337_matches_golden_hex() {
    let output = corpusforge()
        .args([
            "gen",
            "--unicode",
            "invalid-utf8",
            "--output-kind",
            "raw-bytes",
            "--cases",
            "12",
            "--seed",
            "1337",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        bytes_to_hex(&output.stdout),
        fixture("seed_1337_unicode_raw_bytes_invalid_utf8.hex")
    );
    assert!(std::str::from_utf8(&output.stdout).is_err());
    assert_ne!(output.stdout.last(), Some(&b'\n'));
}

#[test]
fn binary_gen_unicode_out_writes_bytes_and_summary() {
    let temp = TestDir::new("unicode-gen-out");
    let out = temp.path().join("unicode.bin");

    let output = corpusforge()
        .args([
            "gen",
            "--unicode",
            "mixed",
            "--output-kind",
            "valid-text",
            "--cases",
            "12",
            "--seed",
            "1337",
            "--out",
        ])
        .arg(&out)
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    let generated = fs::read(&out).expect("unicode output should exist");
    assert_eq!(
        bytes_to_hex(&generated),
        fixture("seed_1337_unicode_valid_text_mixed.hex")
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("generated unicode corpus"));
    assert!(stdout.contains("unicode_mode: mixed"));
    assert!(stdout.contains("output_kind: valid-text"));
    assert!(stdout.contains("case_count: 12"));
    assert!(stdout.contains(&format!("byte_count: {}", generated.len())));
    assert!(stdout.contains("out:"));
}

#[test]
fn binary_gen_unicode_rejects_missing_and_mixed_options() {
    let cases: [(Vec<OsString>, &str); 4] = [
        (
            vec![
                "gen".into(),
                "--unicode".into(),
                "mixed".into(),
                "--cases".into(),
                "12".into(),
                "--seed".into(),
                "1337".into(),
            ],
            "missing required option `--output-kind`",
        ),
        (
            vec![
                "gen".into(),
                "--unicode".into(),
                "mixed".into(),
                "--output-kind".into(),
                "valid-text".into(),
                "--cases".into(),
                "12".into(),
                "--seed".into(),
                "1337".into(),
                "--bytes".into(),
                "64".into(),
            ],
            "cannot be mixed",
        ),
        (
            vec![
                "gen".into(),
                "--unicode".into(),
                "mixed".into(),
                "--output-kind".into(),
                "valid-text".into(),
                "--cases".into(),
                "12".into(),
                "--seed".into(),
                "1337".into(),
                "--json".into(),
            ],
            "only supported for profile-backed",
        ),
        (
            vec![
                "gen".into(),
                "--unicode".into(),
                "mixed".into(),
                "--output-kind".into(),
                "valid-text".into(),
                "--cases".into(),
                "12".into(),
                "--seed".into(),
                "1337".into(),
                "--metadata-out".into(),
                "metadata.json".into(),
            ],
            "only supported for profile-backed",
        ),
    ];

    for (args, expected) in cases {
        assert_invalid_argument(args, expected, expected);
    }
}

#[test]
fn binary_gen_out_writes_exact_bytes_and_summary() {
    let temp = TestDir::new("gen-out");
    let profile = build_repository_profile(&temp);
    let out = temp.path().join("generated.bin");

    let output = corpusforge()
        .args(["gen", "--profile"])
        .arg(&profile)
        .args(["--seed", "1337", "--bytes", "32", "--out"])
        .arg(&out)
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(
        fs::read(&out).expect("generated file should exist").len(),
        32
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("generated corpus"));
    assert!(stdout.contains("byte_count: 32"));
    assert!(stdout.contains("profile_hash: cff:"));
    assert!(stdout.contains("out:"));
}

#[test]
fn binary_gen_metadata_out_writes_stable_json_fields() {
    let temp = TestDir::new("gen-metadata");
    let profile = build_repository_profile(&temp);
    let out = temp.path().join("generated.bin");
    let metadata = temp.path().join("metadata.json");

    let output = corpusforge()
        .args(["gen", "--profile"])
        .arg(&profile)
        .args(["--seed", "1337", "--bytes", "16", "--out"])
        .arg(&out)
        .args(["--metadata-out"])
        .arg(&metadata)
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(
        fs::read(&out).expect("generated file should exist").len(),
        16
    );
    let metadata = fs::read_to_string(&metadata).expect("metadata should be UTF-8");
    assert!(metadata.contains(&format!(
        "\"tool_version\":\"{}\"",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(metadata.contains("\"command\":\"gen\""));
    assert!(metadata.contains("\"seed\":\""));
    assert!(metadata.contains(
        "\"profile_hash\":\"cff:d2fb375e2bda819d4746e0077823653fee6704c314d2c99e40953374add636c6\""
    ));
    assert!(metadata.contains("\"engine_name\":\"corpusforge.byte_bigram\""));
    assert!(metadata.contains("\"engine_version\":0"));
    assert!(metadata.contains("\"byte_count\":16"));
    assert!(metadata.contains("\"determinism\":\"strict\""));
    assert!(metadata.contains("\"output_mode\":\"file\""));
    assert!(metadata.ends_with('\n'));
}

#[test]
fn binary_gen_rejects_profile_without_ngram_model() {
    let temp = TestDir::new("gen-legacy-profile");
    let profile = temp.path().join("legacy.cff");
    let pack = ProfilePack::new(vec![
        ProfileFile::new("legacy.txt", b"legacy".to_vec()).expect("file should be valid")
    ])
    .expect("pack should be valid");
    fs::write(&profile, pack.to_bytes()).expect("legacy profile should be written");

    let output = corpusforge()
        .args(["gen", "--profile"])
        .arg(&profile)
        .args(["--seed", "1337", "--bytes", "8"])
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("NGRAMV0\\0"));
    assert!(stderr.contains("corpusforge profile build"));
}

#[test]
fn binary_malformed_profile_envelope_reports_stable_diagnostic() {
    let temp = TestDir::new("malformed-profile");
    temp.write("bad.cff", &[b'X'; 82]);

    let output = corpusforge()
        .args(["verify", "--profile"])
        .arg(temp.path().join("bad.cff"))
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert_eq!(
        stderr,
        "error: invalid profile: bad .cff magic; expected a CorpusForge .cff binary envelope\n"
    );
}

#[test]
fn binary_profile_malformed_and_unsupported_flags_fail_cleanly() {
    let temp = TestDir::new("profile-errors");
    temp.write("fixture.txt", b"fixture");
    let cases = [
        (
            vec![
                "profile".into(),
                "build".into(),
                "--out".into(),
                "out.cff".into(),
            ],
            "missing input",
        ),
        (
            vec![
                "profile".into(),
                "build".into(),
                temp.path().join("fixture.txt").into_os_string(),
            ],
            "missing required option `--out`",
        ),
        (
            vec![
                "profile".into(),
                "inspect".into(),
                "--profile".into(),
                "x.cff".into(),
                "--format".into(),
                "json".into(),
            ],
            "unknown option `--format`",
        ),
        (
            vec![
                "profile".into(),
                "verify".into(),
                "--profile".into(),
                "x.cff".into(),
                "--unicode".into(),
            ],
            "unknown option `--unicode`",
        ),
        (
            vec![
                "verify".into(),
                "--profile".into(),
                "x.cff".into(),
                "--seed".into(),
                "1".into(),
            ],
            "unknown option `--seed`",
        ),
    ];

    for (args, expected) in cases {
        assert_invalid_argument(args, expected, expected);
    }
}

#[test]
fn binary_malformed_common_flags_fail_cleanly() {
    let cases = [
        (&["gen", "--unknown"][..], "unknown option"),
        (
            &["gen", "--seed", "1", "--seed-file", "seed.txt"][..],
            "conflicts",
        ),
        (
            &["gen", "--determinism", "fast"][..],
            "invalid determinism mode",
        ),
        (&["gen", "--bytes", "0"][..], "greater than zero"),
        (&["gen", "--bytes", "1.5KB"][..], "invalid byte size"),
        (&["gen", "--profile"][..], "missing value"),
    ];

    for (args, expected) in cases {
        assert_invalid_argument(args, &format!("{args:?}"), expected);
    }
}

fn assert_invalid_argument<I, S>(args: I, case: &str, expected: &str)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = corpusforge()
        .args(args)
        .output()
        .expect("binary should run");

    assert!(!output.status.success(), "{case} should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(
        stderr.contains("error: invalid argument"),
        "{case} stderr should be an invalid argument: {stderr}"
    );
    assert!(
        stderr.contains(expected),
        "{case} stderr should contain {expected}: {stderr}"
    );
}

fn assert_profile_summary(stdout: &str) {
    assert!(stdout.contains("version: 0"), "{stdout}");
    assert!(stdout.contains("profile_hash: cff:"), "{stdout}");
    assert!(stdout.contains("file_count:"), "{stdout}");
    assert!(stdout.contains("byte_count:"), "{stdout}");
}

fn expected_fixture_summary(action: &str) -> String {
    format!(
        "{action}\nversion: 0\nprofile_hash: cff:d2fb375e2bda819d4746e0077823653fee6704c314d2c99e40953374add636c6\nfile_count: 3\nbyte_count: 212\n"
    )
}

fn build_repository_profile(temp: &TestDir) -> PathBuf {
    let output_profile = temp.path().join("compiled.cff");
    let build = corpusforge()
        .args(["profile", "build"])
        .arg(repository_fixtures_path())
        .args(["--out"])
        .arg(&output_profile)
        .output()
        .expect("binary should run");
    assert!(build.status.success());
    output_profile
}

fn repository_fixtures_path() -> PathBuf {
    workspace_root().join("tests").join("fixtures")
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hex
}

fn fixture(name: &str) -> &'static str {
    match name {
        "seed_1337_repository_fixtures_ngram.hex" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/golden/seed_1337_repository_fixtures_ngram.hex"
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
        _ => panic!("unknown golden fixture '{name}'"),
    }
}

fn json_escape_for_test(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate should live below workspace crates directory")
        .to_path_buf()
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::current_dir()
            .expect("current directory should be available")
            .join("target")
            .join("corpusforge-cli-tests")
            .join(format!("{}-{id}-{name}", std::process::id()));

        if path.exists() {
            fs::remove_dir_all(&path).expect("stale test directory should be removable");
        }
        fs::create_dir_all(&path).expect("test directory should be created");

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("test parent directory should be created");
        }
        fs::write(path, bytes).expect("test fixture should be written");
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
