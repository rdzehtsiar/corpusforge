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
    }
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
