// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
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
        } else {
            assert!(stdout.contains("--seed <seed>"));
            assert!(stdout.contains("--seed-file <path>"));
            assert!(stdout.contains("--out <path>"));
            assert!(stdout.contains("--bytes <N>"));
            assert!(stdout.contains("--determinism <mode>"));
            assert!(stdout.contains("--metadata-out <path>"));
            assert!(stdout.contains("--quiet"));
            assert!(stdout.contains("--json"));
            assert!(stdout.contains("EXAMPLES"));
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
    assert!(stdout.contains("Planned for a later milestone"));
}

#[test]
fn binary_placeholder_execution_exits_nonzero() {
    let output = corpusforge()
        .arg("gen")
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("error: not implemented"));
    assert!(stderr.contains("gen command execution"));
}

#[test]
fn binary_common_flags_parse_before_placeholder_execution() {
    let output = corpusforge()
        .args([
            "gen",
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
    assert!(stderr.contains("gen command execution"));
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

fn repository_fixtures_path() -> PathBuf {
    workspace_root().join("tests").join("fixtures")
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
