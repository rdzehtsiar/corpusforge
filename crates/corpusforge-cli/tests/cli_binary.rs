// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

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
        assert!(stdout.contains("--seed <seed>"));
        assert!(stdout.contains("--seed-file <path>"));
        assert!(stdout.contains("--profile <path>"));
        assert!(stdout.contains("--out <path>"));
        assert!(stdout.contains("--bytes <N>"));
        assert!(stdout.contains("--determinism <mode>"));
        assert!(stdout.contains("--metadata-out <path>"));
        assert!(stdout.contains("--quiet"));
        assert!(stdout.contains("--json"));
        assert!(stdout.contains("EXAMPLES"));
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
        let output = corpusforge()
            .args(args)
            .output()
            .expect("binary should run");

        assert!(!output.status.success(), "{args:?} should fail");
        let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
        assert!(
            stderr.contains("error: invalid argument"),
            "{stderr} should be an invalid argument"
        );
        assert!(
            stderr.contains(expected),
            "{stderr} should contain {expected}"
        );
    }
}
