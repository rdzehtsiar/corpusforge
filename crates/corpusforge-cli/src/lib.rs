// SPDX-License-Identifier: Apache-2.0

//! Command-line parsing and execution for the CorpusForge binary.

use corpusforge_core::{CorpusForgeError, Result};
use std::ffi::OsString;

const COMMANDS: [CommandSpec; 6] = [
    CommandSpec {
        name: "profile",
        summary: "Inspect and compile CorpusForge profile files",
    },
    CommandSpec {
        name: "gen",
        summary: "Generate deterministic adversarial corpus samples",
    },
    CommandSpec {
        name: "shrink",
        summary: "Minimize a reproducible failing text case",
    },
    CommandSpec {
        name: "replay",
        summary: "Replay a seed, profile, and case range",
    },
    CommandSpec {
        name: "verify",
        summary: "Verify profile and corpus compatibility metadata",
    },
    CommandSpec {
        name: "ci",
        summary: "Run CI-friendly corpus checks and reports",
    },
];

/// Successful command output or a planned-feature error.
#[derive(Debug)]
pub enum CliOutcome {
    /// Text to print to standard output.
    Success(String),
    /// A clean project error to print to standard error.
    Failure(CorpusForgeError),
}

#[derive(Debug, Eq, PartialEq)]
enum ParsedCommand {
    TopHelp,
    Version,
    CommandHelp(&'static CommandSpec),
    Execute(&'static CommandSpec),
}

#[derive(Debug, Eq, PartialEq)]
struct CommandSpec {
    name: &'static str,
    summary: &'static str,
}

/// Parses arguments and returns the CLI outcome without writing to the terminal.
pub fn run<I, S>(args: I) -> CliOutcome
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match parse(args) {
        Ok(command) => match command {
            ParsedCommand::TopHelp => CliOutcome::Success(top_level_help()),
            ParsedCommand::Version => CliOutcome::Success(version_text()),
            ParsedCommand::CommandHelp(command) => CliOutcome::Success(command_help(command)),
            ParsedCommand::Execute(command) => CliOutcome::Failure(
                CorpusForgeError::not_implemented(format!("{} command execution", command.name)),
            ),
        },
        Err(error) => CliOutcome::Failure(error),
    }
}

/// Returns process exit code for an outcome and writes it to the provided streams.
pub fn write_outcome(
    outcome: CliOutcome,
    stdout: &mut impl std::io::Write,
    stderr: &mut impl std::io::Write,
) -> i32 {
    match outcome {
        CliOutcome::Success(text) => {
            let _ = writeln!(stdout, "{text}");
            0
        }
        CliOutcome::Failure(error) => {
            let _ = writeln!(stderr, "error: {error}");
            1
        }
    }
}

fn parse<I, S>(args: I) -> Result<ParsedCommand>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let _program = args.next();

    let Some(first) = args.next() else {
        return Ok(ParsedCommand::TopHelp);
    };

    let first = first.to_string_lossy();
    match first.as_ref() {
        "--help" | "-h" => Ok(ParsedCommand::TopHelp),
        "--version" | "-V" => Ok(ParsedCommand::Version),
        command_name => {
            let command = find_command(command_name).ok_or_else(|| {
                CorpusForgeError::invalid_profile(format!(
                    "unknown command `{command_name}`; run `corpusforge --help`"
                ))
            })?;

            let remaining = args.collect::<Vec<_>>();
            if contains_help_flag(&remaining) {
                Ok(ParsedCommand::CommandHelp(command))
            } else {
                parse_common_options(command, &remaining)?;
                Ok(ParsedCommand::Execute(command))
            }
        }
    }
}

fn contains_help_flag(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn parse_common_options(command: &CommandSpec, args: &[OsString]) -> Result<()> {
    let mut state = CommonOptionState::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();

        match flag.as_ref() {
            "--seed" => {
                state.claim_seed("--seed")?;
                let value = take_value(command, args, index, "--seed")?;
                validate_non_empty(&value, "--seed")?;
                index += 2;
            }
            "--seed-file" => {
                state.claim_seed("--seed-file")?;
                let value = take_value(command, args, index, "--seed-file")?;
                validate_non_empty(&value, "--seed-file")?;
                index += 2;
            }
            "--profile" => {
                state.claim_once("profile", "--profile")?;
                let value = take_value(command, args, index, "--profile")?;
                validate_non_empty(&value, "--profile")?;
                index += 2;
            }
            "--out" => {
                state.claim_once("out", "--out")?;
                let value = take_value(command, args, index, "--out")?;
                validate_non_empty(&value, "--out")?;
                index += 2;
            }
            "--bytes" => {
                state.claim_once("bytes", "--bytes")?;
                let value = take_value(command, args, index, "--bytes")?;
                parse_byte_size(&value)?;
                index += 2;
            }
            "--determinism" => {
                state.claim_once("determinism", "--determinism")?;
                let value = take_value(command, args, index, "--determinism")?;
                parse_determinism(&value)?;
                index += 2;
            }
            "--metadata-out" => {
                state.claim_once("metadata_out", "--metadata-out")?;
                let value = take_value(command, args, index, "--metadata-out")?;
                validate_non_empty(&value, "--metadata-out")?;
                index += 2;
            }
            "--quiet" => {
                state.claim_once("quiet", "--quiet")?;
                index += 1;
            }
            "--json" => {
                state.claim_once("json", "--json")?;
                index += 1;
            }
            "-h" | "--help" => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "help must be requested without other arguments; run `corpusforge {} --help`",
                    command.name
                )));
            }
            other if other.starts_with('-') => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unknown option `{other}` for `{}`; run `corpusforge {} --help`",
                    command.name, command.name
                )));
            }
            other => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unexpected argument `{other}` for `{}`; run `corpusforge {} --help`",
                    command.name, command.name
                )));
            }
        }
    }

    Ok(())
}

#[derive(Default)]
struct CommonOptionState {
    seed_source: Option<&'static str>,
    profile: bool,
    out: bool,
    bytes: bool,
    determinism: bool,
    metadata_out: bool,
    quiet: bool,
    json: bool,
}

impl CommonOptionState {
    fn claim_seed(&mut self, flag: &'static str) -> Result<()> {
        if let Some(existing) = self.seed_source {
            return Err(CorpusForgeError::invalid_argument(format!(
                "seed input `{flag}` conflicts with `{existing}`"
            )));
        }

        self.seed_source = Some(flag);
        Ok(())
    }

    fn claim_once(&mut self, field: &str, flag: &'static str) -> Result<()> {
        let used = match field {
            "profile" => &mut self.profile,
            "out" => &mut self.out,
            "bytes" => &mut self.bytes,
            "determinism" => &mut self.determinism,
            "metadata_out" => &mut self.metadata_out,
            "quiet" => &mut self.quiet,
            "json" => &mut self.json,
            _ => unreachable!("unknown CLI option state field"),
        };

        if *used {
            return Err(CorpusForgeError::invalid_argument(format!(
                "duplicate option `{flag}`"
            )));
        }

        *used = true;
        Ok(())
    }
}

fn take_value(
    command: &CommandSpec,
    args: &[OsString],
    flag_index: usize,
    flag: &str,
) -> Result<String> {
    let Some(value) = args.get(flag_index + 1) else {
        return Err(CorpusForgeError::invalid_argument(format!(
            "missing value for `{flag}`; run `corpusforge {} --help`",
            command.name
        )));
    };

    let value = value.to_string_lossy();
    if value.starts_with('-') {
        return Err(CorpusForgeError::invalid_argument(format!(
            "missing value for `{flag}`; run `corpusforge {} --help`",
            command.name
        )));
    }

    Ok(value.into_owned())
}

fn validate_non_empty(value: &str, flag: &str) -> Result<()> {
    if value.is_empty() {
        return Err(CorpusForgeError::invalid_argument(format!(
            "`{flag}` requires a non-empty value"
        )));
    }

    Ok(())
}

fn parse_determinism(value: &str) -> Result<()> {
    match value {
        "strict" | "relaxed" => Ok(()),
        _ => Err(CorpusForgeError::invalid_argument(format!(
            "invalid determinism mode `{value}`; expected `strict` or `relaxed`"
        ))),
    }
}

fn parse_byte_size(value: &str) -> Result<u64> {
    let (digits, multiplier) = if let Some(digits) = strip_ascii_suffix(value, "KB") {
        (digits, 1024_u64)
    } else if let Some(digits) = strip_ascii_suffix(value, "MB") {
        (digits, 1024_u64.pow(2))
    } else if let Some(digits) = strip_ascii_suffix(value, "GB") {
        (digits, 1024_u64.pow(3))
    } else {
        (value, 1)
    };

    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CorpusForgeError::invalid_argument(format!(
            "invalid byte size `{value}`; expected a positive integer with optional KB, MB, or GB suffix"
        )));
    }

    let parsed = digits.parse::<u64>().map_err(|_| {
        CorpusForgeError::invalid_argument(format!(
            "invalid byte size `{value}`; expected a positive integer with optional KB, MB, or GB suffix"
        ))
    })?;

    let bytes = parsed.checked_mul(multiplier).ok_or_else(|| {
        CorpusForgeError::invalid_argument(format!("byte size `{value}` is too large"))
    })?;

    if bytes == 0 {
        return Err(CorpusForgeError::invalid_argument(
            "byte size must be greater than zero",
        ));
    }

    Ok(bytes)
}

fn strip_ascii_suffix<'a>(value: &'a str, suffix: &str) -> Option<&'a str> {
    value
        .get(value.len().checked_sub(suffix.len())?..)
        .filter(|tail| tail.eq_ignore_ascii_case(suffix))
        .map(|_| &value[..value.len() - suffix.len()])
}

fn find_command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|command| command.name == name)
}

fn top_level_help() -> String {
    let mut help = format!(
        "{name} {version}\n\n{about}\n\nUSAGE:\n    corpusforge <COMMAND>\n\nCOMMANDS:\n",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        about = env!("CARGO_PKG_DESCRIPTION")
    );

    for command in COMMANDS {
        help.push_str(&format!("    {:<8} {}\n", command.name, command.summary));
    }

    help.push_str(
        "\nOPTIONS:\n    -h, --help       Print help\n    -V, --version    Print version\n",
    );
    help
}

fn command_help(command: &CommandSpec) -> String {
    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge {name} [OPTIONS]\n\nOPTIONS:\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a file\n    --profile <path>              Read a CorpusForge profile path\n    --out <path>                  Write generated output to a path\n    --bytes <N>                   Set output size in bytes; supports KB, MB, GB\n    --determinism <mode>          Determinism mode: strict or relaxed\n    --metadata-out <path>         Write machine-readable metadata to a path\n    --quiet                       Reduce human-readable output\n    --json                        Emit machine-readable JSON where supported\n    -h, --help                    Print help\n\nEXAMPLES:\n    corpusforge {name} --seed 42 --profile profiles/smoke.cff --bytes 64KB\n    corpusforge {name} --seed-file seed.txt --determinism strict --metadata-out report.json --json\n\nSTATUS:\n    Planned for a later milestone; execution currently returns NotImplemented.",
        name = command.name,
        summary = command.summary
    )
}

fn version_text() -> String {
    format!(
        "corpusforge {version} ({profile})",
        version = env!("CARGO_PKG_VERSION"),
        profile = build_profile()
    )
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

#[cfg(test)]
mod tests {
    use super::{run, CliOutcome};

    #[test]
    fn top_level_help_lists_commands() {
        let CliOutcome::Success(help) = run(["corpusforge", "--help"]) else {
            panic!("help should succeed");
        };

        for command in ["profile", "gen", "shrink", "replay", "verify", "ci"] {
            assert!(help.contains(command), "help should list {command}");
        }
        assert!(help.contains("--version"));
    }

    #[test]
    fn short_help_matches_top_level_help_behavior() {
        let CliOutcome::Success(long) = run(["corpusforge", "--help"]) else {
            panic!("long help should succeed");
        };
        let CliOutcome::Success(short) = run(["corpusforge", "-h"]) else {
            panic!("short help should succeed");
        };

        assert_eq!(short, long);
    }

    #[test]
    fn version_includes_crate_version_and_profile() {
        let CliOutcome::Success(version) = run(["corpusforge", "--version"]) else {
            panic!("version should succeed");
        };

        assert!(version.contains(env!("CARGO_PKG_VERSION")));
        assert!(version.contains("debug") || version.contains("release"));
    }

    #[test]
    fn command_help_succeeds_for_all_commands() {
        for command in ["profile", "gen", "shrink", "replay", "verify", "ci"] {
            let CliOutcome::Success(help) = run(["corpusforge", command, "--help"]) else {
                panic!("{command} help should succeed");
            };

            assert!(help.contains(&format!("corpusforge {command}")));
            assert!(help.contains("--seed <seed>"));
            assert!(help.contains("--seed-file <path>"));
            assert!(help.contains("--profile <path>"));
            assert!(help.contains("--out <path>"));
            assert!(help.contains("--bytes <N>"));
            assert!(help.contains("--determinism <mode>"));
            assert!(help.contains("--metadata-out <path>"));
            assert!(help.contains("--quiet"));
            assert!(help.contains("--json"));
            assert!(help.contains("EXAMPLES"));
            assert!(help.contains("Planned for a later milestone"));
        }
    }

    #[test]
    fn command_help_succeeds_alongside_common_flags() {
        let CliOutcome::Success(help) = run([
            "corpusforge",
            "gen",
            "--seed",
            "1337",
            "--bytes",
            "1MB",
            "--help",
        ]) else {
            panic!("gen help with common flags should succeed");
        };

        assert!(help.contains("corpusforge gen"));
        assert!(help.contains("--bytes <N>"));
        assert!(help.contains("Planned for a later milestone"));
    }

    #[test]
    fn placeholder_command_returns_not_implemented() {
        let CliOutcome::Failure(error) = run(["corpusforge", "gen"]) else {
            panic!("placeholder execution should fail");
        };

        assert_eq!(error.category(), "not_implemented");
        assert!(error.to_string().contains("gen command execution"));
    }

    #[test]
    fn common_options_parse_before_placeholder_execution() {
        let CliOutcome::Failure(error) = run([
            "corpusforge",
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
        ]) else {
            panic!("placeholder execution should fail");
        };

        assert_eq!(error.category(), "not_implemented");
        assert!(error.to_string().contains("gen command execution"));
    }

    #[test]
    fn seed_file_option_parse_before_placeholder_execution() {
        let CliOutcome::Failure(error) = run([
            "corpusforge",
            "replay",
            "--seed-file",
            "seed.txt",
            "--bytes",
            "1GB",
            "--determinism",
            "relaxed",
        ]) else {
            panic!("placeholder execution should fail");
        };

        assert_eq!(error.category(), "not_implemented");
        assert!(error.to_string().contains("replay command execution"));
    }

    #[test]
    fn plain_byte_size_parses_before_placeholder_execution() {
        let CliOutcome::Failure(error) =
            run(["corpusforge", "gen", "--seed", "42", "--bytes", "1024"])
        else {
            panic!("placeholder execution should fail");
        };

        assert_eq!(error.category(), "not_implemented");
        assert!(error.to_string().contains("gen command execution"));
    }

    #[test]
    fn malformed_common_options_fail_cleanly() {
        let cases = [
            (&["corpusforge", "gen", "--unknown"][..], "unknown option"),
            (
                &["corpusforge", "gen", "--seed", "1", "--seed", "2"][..],
                "conflicts",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--seed",
                    "1",
                    "--seed-file",
                    "seed.txt",
                ][..],
                "conflicts",
            ),
            (
                &["corpusforge", "gen", "--determinism", "fast"][..],
                "invalid determinism mode",
            ),
            (
                &["corpusforge", "gen", "--bytes", "0"][..],
                "greater than zero",
            ),
            (
                &["corpusforge", "gen", "--bytes", "1.5KB"][..],
                "invalid byte size",
            ),
            (&["corpusforge", "gen", "--profile"][..], "missing value"),
            (
                &["corpusforge", "gen", "--profile", "--json"][..],
                "missing value",
            ),
        ];

        for (args, expected) in cases {
            let CliOutcome::Failure(error) = run(args) else {
                panic!("{args:?} should fail");
            };

            assert_eq!(error.category(), "invalid_argument");
            assert!(
                error.to_string().contains(expected),
                "{error} should contain {expected}"
            );
        }
    }

    #[test]
    fn unknown_command_fails_cleanly() {
        let CliOutcome::Failure(error) = run(["corpusforge", "unknown"]) else {
            panic!("unknown command should fail");
        };

        assert_eq!(error.category(), "invalid_profile");
        assert!(error.to_string().contains("unknown command"));
    }
}
