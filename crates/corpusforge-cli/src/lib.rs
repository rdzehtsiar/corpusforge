// SPDX-License-Identifier: Apache-2.0

//! Command-line parsing and execution for the CorpusForge binary.

use corpusforge_cff::{InspectSummary, ProfilePack};
use corpusforge_core::seed::MasterSeed;
use corpusforge_core::{CorpusForgeError, Result};
use corpusforge_ngram::{ByteBigramModel, ENGINE_NAME, ENGINE_VERSION};
use corpusforge_profile::compile_path;
use corpusforge_tokenizer::{
    generate_tokenizer_cases, run_stdin_harness, HarnessCommand, HarnessStatus, TokenizerCaseSpec,
    TokenizerReport, UnicodeMode, UnicodeOutputKind,
};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

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
    /// Binary bytes to write to standard output without a trailing newline.
    SuccessBytes(Vec<u8>),
    /// A clean project error to print to standard error.
    Failure(CorpusForgeError),
}

impl From<Result<String>> for CliOutcome {
    fn from(result: Result<String>) -> Self {
        match result {
            Ok(text) => Self::Success(text),
            Err(error) => Self::Failure(error),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ParsedCommand {
    TopHelp,
    Version,
    CommandHelp(&'static CommandSpec),
    ExecutePlaceholder(&'static CommandSpec),
    ExecuteCiTokenizer(CiTokenizerOptions),
    ExecuteGen(GenOptions),
    ExecuteProfile(ProfileCommand),
    ExecuteVerifyAlias(ProfileFileOptions),
}

#[derive(Debug, Eq, PartialEq)]
struct CommandSpec {
    name: &'static str,
    summary: &'static str,
}

#[derive(Debug, Eq, PartialEq)]
enum ProfileCommand {
    Build(ProfileBuildOptions),
    Inspect(ProfileFileOptions),
    Verify(ProfileFileOptions),
}

#[derive(Debug, Eq, PartialEq)]
struct ProfileBuildOptions {
    input: PathBuf,
    out: PathBuf,
    json: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct ProfileFileOptions {
    profile: PathBuf,
    json: bool,
}

#[derive(Debug, Eq, PartialEq)]
enum GenOptions {
    Profile(ProfileGenOptions),
    Unicode(UnicodeGenOptions),
}

#[derive(Debug, Eq, PartialEq)]
struct ProfileGenOptions {
    profile: PathBuf,
    seed_source: SeedSource,
    byte_count: usize,
    out: Option<PathBuf>,
    metadata_out: Option<PathBuf>,
    determinism: DeterminismMode,
    quiet: bool,
    json: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct UnicodeGenOptions {
    mode: UnicodeMode,
    output_kind: UnicodeOutputKind,
    case_count: usize,
    seed_source: SeedSource,
    out: Option<PathBuf>,
    quiet: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct CiTokenizerOptions {
    mode: UnicodeMode,
    output_kind: UnicodeOutputKind,
    case_count: usize,
    seed_source: SeedSource,
    command: PathBuf,
    args: Vec<String>,
    report_out: PathBuf,
}

#[derive(Debug, Eq, PartialEq)]
enum SeedSource {
    Inline(String),
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeterminismMode {
    Strict,
    Relaxed,
}

impl DeterminismMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Relaxed => "relaxed",
        }
    }
}

/// Parses arguments and returns the CLI outcome without writing to the terminal.
pub fn run<I, S>(args: I) -> CliOutcome
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match parse(args) {
        Ok(command) => {
            let mut stdout = Vec::new();
            match execute_command(command, &mut stdout) {
                Ok(Some(text)) => CliOutcome::Success(text),
                Ok(None) => CliOutcome::SuccessBytes(stdout),
                Err(error) => CliOutcome::Failure(error),
            }
        }
        Err(error) => CliOutcome::Failure(error),
    }
}

/// Parses and executes a command using caller-provided streams.
pub fn run_to_writers<I, S>(
    args: I,
    stdout: &mut impl std::io::Write,
    stderr: &mut impl std::io::Write,
) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match parse(args).and_then(|command| execute_command(command, stdout)) {
        Ok(Some(text)) => {
            if !text.is_empty() {
                let _ = writeln!(stdout, "{text}");
            }
            0
        }
        Ok(None) => 0,
        Err(error) => {
            let _ = writeln!(stderr, "error: {error}");
            1
        }
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
            if !text.is_empty() {
                let _ = writeln!(stdout, "{text}");
            }
            0
        }
        CliOutcome::SuccessBytes(bytes) => {
            let _ = stdout.write_all(&bytes);
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
            if command.name == "ci" && first_arg_is(&remaining, "tokenizer") {
                let tokenizer_args = &remaining[1..];
                if contains_only_help_flag(tokenizer_args) {
                    Ok(ParsedCommand::CommandHelp(command))
                } else {
                    parse_ci_tokenizer_options(tokenizer_args)
                        .map(ParsedCommand::ExecuteCiTokenizer)
                }
            } else if contains_help_flag(&remaining) {
                Ok(ParsedCommand::CommandHelp(command))
            } else if command.name == "profile" {
                parse_profile_command(&remaining).map(ParsedCommand::ExecuteProfile)
            } else if command.name == "gen" {
                parse_gen_options(&remaining).map(ParsedCommand::ExecuteGen)
            } else if command.name == "verify" && contains_profile_option(&remaining) {
                parse_profile_file_options("verify", &remaining)
                    .map(ParsedCommand::ExecuteVerifyAlias)
            } else {
                parse_common_options(command, &remaining)?;
                Ok(ParsedCommand::ExecutePlaceholder(command))
            }
        }
    }
}

fn contains_help_flag(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn contains_only_help_flag(args: &[OsString]) -> bool {
    matches!(args, [arg] if arg == "--help" || arg == "-h")
}

fn contains_profile_option(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == "--profile")
}

fn first_arg_is(args: &[OsString], expected: &str) -> bool {
    args.first()
        .is_some_and(|arg| arg.to_string_lossy() == expected)
}

fn parse_profile_command(args: &[OsString]) -> Result<ProfileCommand> {
    let Some(subcommand) = args.first() else {
        return Err(CorpusForgeError::invalid_argument(
            "missing profile subcommand; expected `build`, `inspect`, or `verify`",
        ));
    };

    let subcommand = subcommand.to_string_lossy();
    match subcommand.as_ref() {
        "build" => parse_profile_build_options(&args[1..]).map(ProfileCommand::Build),
        "inspect" => {
            parse_profile_file_options("profile inspect", &args[1..]).map(ProfileCommand::Inspect)
        }
        "verify" => {
            parse_profile_file_options("profile verify", &args[1..]).map(ProfileCommand::Verify)
        }
        other if other.starts_with('-') => Err(CorpusForgeError::invalid_argument(format!(
            "missing profile subcommand before option `{other}`; expected `build`, `inspect`, or `verify`"
        ))),
        other => Err(CorpusForgeError::invalid_argument(format!(
            "unknown profile subcommand `{other}`; expected `build`, `inspect`, or `verify`"
        ))),
    }
}

fn parse_profile_build_options(args: &[OsString]) -> Result<ProfileBuildOptions> {
    let mut input = None;
    let mut out = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        let text = arg.to_string_lossy();

        match text.as_ref() {
            "--out" => {
                if out.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--out`",
                    ));
                }
                out = Some(take_path_value("profile build", args, index, "--out")?);
                index += 2;
            }
            "--json" => {
                if json {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--json`",
                    ));
                }
                json = true;
                index += 1;
            }
            other if other.starts_with('-') => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unknown option `{other}` for `profile build`; run `corpusforge profile --help`"
                )));
            }
            _ => {
                if input.is_some() {
                    return Err(CorpusForgeError::invalid_argument(format!(
                        "unexpected argument `{text}` for `profile build`; expected one input path"
                    )));
                }
                input = Some(PathBuf::from(arg.as_os_str()));
                index += 1;
            }
        }
    }

    let input = input.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing input path for `profile build`")
    })?;
    let out = out.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--out` for `profile build`")
    })?;

    Ok(ProfileBuildOptions { input, out, json })
}

fn parse_profile_file_options(command: &str, args: &[OsString]) -> Result<ProfileFileOptions> {
    let mut profile = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        let text = arg.to_string_lossy();

        match text.as_ref() {
            "--profile" => {
                if profile.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--profile`",
                    ));
                }
                profile = Some(take_path_value(command, args, index, "--profile")?);
                index += 2;
            }
            "--json" => {
                if json {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--json`",
                    ));
                }
                json = true;
                index += 1;
            }
            other if other.starts_with('-') => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unknown option `{other}` for `{command}`; run `corpusforge {command} --help`"
                )));
            }
            _ => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unexpected argument `{text}` for `{command}`; use `--profile <path>`"
                )));
            }
        }
    }

    let profile = profile.ok_or_else(|| {
        CorpusForgeError::invalid_argument(format!(
            "missing required option `--profile` for `{command}`"
        ))
    })?;

    Ok(ProfileFileOptions { profile, json })
}

fn parse_gen_options(args: &[OsString]) -> Result<GenOptions> {
    let command = find_command("gen").expect("gen command should exist");
    let mut profile = None;
    let mut seed_source = None;
    let mut byte_count = None;
    let mut unicode_mode = None;
    let mut output_kind = None;
    let mut case_count = None;
    let mut out = None;
    let mut metadata_out = None;
    let mut determinism = DeterminismMode::Strict;
    let mut determinism_set = false;
    let mut quiet = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();

        match flag.as_ref() {
            "--profile" => {
                if profile.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--profile`",
                    ));
                }
                profile = Some(take_path_value("gen", args, index, "--profile")?);
                index += 2;
            }
            "--seed" => {
                if let Some(existing) = seed_source_name(&seed_source) {
                    return Err(seed_conflict("--seed", existing));
                }
                let seed = take_value(command, args, index, "--seed")?;
                validate_non_empty(&seed, "--seed")?;
                seed_source = Some(SeedSource::Inline(seed));
                index += 2;
            }
            "--seed-file" => {
                if let Some(existing) = seed_source_name(&seed_source) {
                    return Err(seed_conflict("--seed-file", existing));
                }
                seed_source = Some(SeedSource::File(take_path_value(
                    "gen",
                    args,
                    index,
                    "--seed-file",
                )?));
                index += 2;
            }
            "--bytes" => {
                if byte_count.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--bytes`",
                    ));
                }
                let value = take_value(command, args, index, "--bytes")?;
                let bytes = parse_byte_size(&value)?;
                byte_count = Some(usize::try_from(bytes).map_err(|_| {
                    CorpusForgeError::invalid_argument(format!(
                        "byte size `{value}` exceeds this platform's maximum supported output size"
                    ))
                })?);
                index += 2;
            }
            "--unicode" => {
                if unicode_mode.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--unicode`",
                    ));
                }
                let value = take_value(command, args, index, "--unicode")?;
                unicode_mode = Some(UnicodeMode::from_str(&value)?);
                index += 2;
            }
            "--output-kind" => {
                if output_kind.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--output-kind`",
                    ));
                }
                let value = take_value(command, args, index, "--output-kind")?;
                output_kind = Some(UnicodeOutputKind::from_str(&value)?);
                index += 2;
            }
            "--cases" => {
                if case_count.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--cases`",
                    ));
                }
                let value = take_value(command, args, index, "--cases")?;
                case_count = Some(parse_case_count(&value)?);
                index += 2;
            }
            "--out" => {
                if out.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--out`",
                    ));
                }
                out = Some(take_path_value("gen", args, index, "--out")?);
                index += 2;
            }
            "--metadata-out" => {
                if metadata_out.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--metadata-out`",
                    ));
                }
                metadata_out = Some(take_path_value("gen", args, index, "--metadata-out")?);
                index += 2;
            }
            "--determinism" => {
                if determinism_set {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--determinism`",
                    ));
                }
                let value = take_value(command, args, index, "--determinism")?;
                determinism = parse_determinism_mode(&value)?;
                determinism_set = true;
                index += 2;
            }
            "--quiet" => {
                if quiet {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--quiet`",
                    ));
                }
                quiet = true;
                index += 1;
            }
            "--json" => {
                if json {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--json`",
                    ));
                }
                json = true;
                index += 1;
            }
            "-h" | "--help" => {
                return Err(CorpusForgeError::invalid_argument(
                    "help must be requested without other arguments; run `corpusforge gen --help`",
                ));
            }
            other if other.starts_with('-') => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unknown option `{other}` for `gen`; run `corpusforge gen --help`"
                )));
            }
            other => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unexpected argument `{other}` for `gen`; run `corpusforge gen --help`"
                )));
            }
        }
    }

    let uses_profile_path = profile.is_some() || byte_count.is_some();
    let uses_unicode_path = unicode_mode.is_some() || output_kind.is_some() || case_count.is_some();

    if uses_profile_path && uses_unicode_path {
        return Err(CorpusForgeError::invalid_argument(
            "`gen` profile/bytes options cannot be mixed with `--unicode`, `--output-kind`, or `--cases`",
        ));
    }

    if uses_unicode_path {
        if json {
            return Err(CorpusForgeError::invalid_argument(
                "`--json` is only supported for profile-backed `gen --out`",
            ));
        }

        if metadata_out.is_some() {
            return Err(CorpusForgeError::invalid_argument(
                "`--metadata-out` is only supported for profile-backed `gen`",
            ));
        }

        if determinism_set {
            return Err(CorpusForgeError::invalid_argument(
                "`--determinism` is only supported for profile-backed `gen`",
            ));
        }

        let mode = unicode_mode.ok_or_else(|| {
            CorpusForgeError::invalid_argument("missing required option `--unicode` for `gen`")
        })?;
        let output_kind = output_kind.ok_or_else(|| {
            CorpusForgeError::invalid_argument("missing required option `--output-kind` for `gen`")
        })?;
        let case_count = case_count.ok_or_else(|| {
            CorpusForgeError::invalid_argument("missing required option `--cases` for `gen`")
        })?;
        let seed_source = seed_source.ok_or_else(|| {
            CorpusForgeError::invalid_argument(
                "missing required seed source for `gen`; use exactly one of `--seed` or `--seed-file`",
            )
        })?;

        TokenizerCaseSpec::new(mode, output_kind, case_count)?;

        return Ok(GenOptions::Unicode(UnicodeGenOptions {
            mode,
            output_kind,
            case_count,
            seed_source,
            out,
            quiet,
        }));
    }

    if json && out.is_none() {
        return Err(CorpusForgeError::invalid_argument(
            "`--json` requires `--out` for `gen` because standard output carries generated binary bytes",
        ));
    }

    let profile = profile.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--profile` for `gen`")
    })?;
    let seed_source = seed_source.ok_or_else(|| {
        CorpusForgeError::invalid_argument(
            "missing required seed source for `gen`; use exactly one of `--seed` or `--seed-file`",
        )
    })?;
    let byte_count = byte_count.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--bytes` for `gen`")
    })?;

    Ok(GenOptions::Profile(ProfileGenOptions {
        profile,
        seed_source,
        byte_count,
        out,
        metadata_out,
        determinism,
        quiet,
        json,
    }))
}

fn parse_ci_tokenizer_options(args: &[OsString]) -> Result<CiTokenizerOptions> {
    let command_spec = find_command("ci").expect("ci command should exist");
    let mut unicode_mode = None;
    let mut output_kind = None;
    let mut case_count = None;
    let mut seed_source = None;
    let mut command = None;
    let mut harness_args = Vec::new();
    let mut report_out = None;
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();

        match flag.as_ref() {
            "--unicode" => {
                if unicode_mode.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--unicode`",
                    ));
                }
                let value = take_value(command_spec, args, index, "--unicode")?;
                unicode_mode = Some(UnicodeMode::from_str(&value)?);
                index += 2;
            }
            "--output-kind" => {
                if output_kind.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--output-kind`",
                    ));
                }
                let value = take_value(command_spec, args, index, "--output-kind")?;
                output_kind = Some(UnicodeOutputKind::from_str(&value)?);
                index += 2;
            }
            "--cases" => {
                if case_count.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--cases`",
                    ));
                }
                let value = take_value(command_spec, args, index, "--cases")?;
                case_count = Some(parse_case_count(&value)?);
                index += 2;
            }
            "--seed" => {
                if let Some(existing) = seed_source_name(&seed_source) {
                    return Err(seed_conflict("--seed", existing));
                }
                let seed = take_value(command_spec, args, index, "--seed")?;
                validate_non_empty(&seed, "--seed")?;
                seed_source = Some(SeedSource::Inline(seed));
                index += 2;
            }
            "--seed-file" => {
                if let Some(existing) = seed_source_name(&seed_source) {
                    return Err(seed_conflict("--seed-file", existing));
                }
                seed_source = Some(SeedSource::File(take_path_value(
                    "ci",
                    args,
                    index,
                    "--seed-file",
                )?));
                index += 2;
            }
            "--command" => {
                if command.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--command`",
                    ));
                }
                command = Some(take_path_value("ci", args, index, "--command")?);
                index += 2;
            }
            "--arg" => {
                harness_args.push(take_raw_string_value("ci", args, index, "--arg")?);
                index += 2;
            }
            "--report-out" => {
                if report_out.is_some() {
                    return Err(CorpusForgeError::invalid_argument(
                        "duplicate option `--report-out`",
                    ));
                }
                report_out = Some(take_path_value("ci", args, index, "--report-out")?);
                index += 2;
            }
            "-h" | "--help" => {
                return Err(CorpusForgeError::invalid_argument(
                    "help must be requested without other arguments; run `corpusforge ci --help`",
                ));
            }
            other if other.starts_with('-') => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unknown option `{other}` for `ci tokenizer`; run `corpusforge ci --help`"
                )));
            }
            other => {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "unexpected argument `{other}` for `ci tokenizer`; run `corpusforge ci --help`"
                )));
            }
        }
    }

    let mode = unicode_mode.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--unicode` for `ci tokenizer`")
    })?;
    let output_kind = output_kind.ok_or_else(|| {
        CorpusForgeError::invalid_argument(
            "missing required option `--output-kind` for `ci tokenizer`",
        )
    })?;
    let case_count = case_count.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--cases` for `ci tokenizer`")
    })?;
    let seed_source = seed_source.ok_or_else(|| {
        CorpusForgeError::invalid_argument(
            "missing required seed source for `ci tokenizer`; use exactly one of `--seed` or `--seed-file`",
        )
    })?;
    let command = command.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--command` for `ci tokenizer`")
    })?;
    let report_out = report_out.ok_or_else(|| {
        CorpusForgeError::invalid_argument(
            "missing required option `--report-out` for `ci tokenizer`",
        )
    })?;

    TokenizerCaseSpec::new(mode, output_kind, case_count)?;

    Ok(CiTokenizerOptions {
        mode,
        output_kind,
        case_count,
        seed_source,
        command,
        args: harness_args,
        report_out,
    })
}

fn seed_source_name(seed_source: &Option<SeedSource>) -> Option<&'static str> {
    match seed_source {
        Some(SeedSource::Inline(_)) => Some("--seed"),
        Some(SeedSource::File(_)) => Some("--seed-file"),
        None => None,
    }
}

fn seed_conflict(flag: &'static str, existing: &'static str) -> CorpusForgeError {
    CorpusForgeError::invalid_argument(format!("seed input `{flag}` conflicts with `{existing}`"))
}

fn take_path_value(
    command: &str,
    args: &[OsString],
    flag_index: usize,
    flag: &str,
) -> Result<PathBuf> {
    let Some(value) = args.get(flag_index + 1) else {
        return Err(CorpusForgeError::invalid_argument(format!(
            "missing value for `{flag}`; run `corpusforge {command} --help`"
        )));
    };

    let text = value.to_string_lossy();
    if text.is_empty() {
        return Err(CorpusForgeError::invalid_argument(format!(
            "`{flag}` requires a non-empty value"
        )));
    }

    if text.starts_with('-') {
        return Err(CorpusForgeError::invalid_argument(format!(
            "missing value for `{flag}`; run `corpusforge {command} --help`"
        )));
    }

    Ok(PathBuf::from(value.as_os_str()))
}

fn take_raw_string_value(
    command: &str,
    args: &[OsString],
    flag_index: usize,
    flag: &str,
) -> Result<String> {
    let Some(value) = args.get(flag_index + 1) else {
        return Err(CorpusForgeError::invalid_argument(format!(
            "missing value for `{flag}`; run `corpusforge {command} --help`"
        )));
    };

    let value = value.to_string_lossy();
    if value.is_empty() {
        return Err(CorpusForgeError::invalid_argument(format!(
            "`{flag}` requires a non-empty value"
        )));
    }

    Ok(value.into_owned())
}

fn execute_command(
    command: ParsedCommand,
    stdout: &mut impl std::io::Write,
) -> Result<Option<String>> {
    match command {
        ParsedCommand::TopHelp => Ok(Some(top_level_help())),
        ParsedCommand::Version => Ok(Some(version_text())),
        ParsedCommand::CommandHelp(command) => Ok(Some(command_help(command))),
        ParsedCommand::ExecutePlaceholder(command) => Err(CorpusForgeError::not_implemented(
            format!("{} command execution", command.name),
        )),
        ParsedCommand::ExecuteCiTokenizer(options) => execute_ci_tokenizer(options).map(Some),
        ParsedCommand::ExecuteGen(options) => execute_gen(options, stdout),
        ParsedCommand::ExecuteProfile(command) => execute_profile_command(command).map(Some),
        ParsedCommand::ExecuteVerifyAlias(options) => execute_profile_verify(options).map(Some),
    }
}

fn execute_ci_tokenizer(options: CiTokenizerOptions) -> Result<String> {
    let seed = read_seed(&options.seed_source)?;
    let spec = TokenizerCaseSpec::new(options.mode, options.output_kind, options.case_count)?;
    let cases = generate_tokenizer_cases(&seed, spec)?;
    let harness_command = HarnessCommand::new(options.command.clone(), options.args.clone());
    let run = run_stdin_harness(&harness_command, &cases);
    let status = run.status();
    let report = TokenizerReport::new(
        env!("CARGO_PKG_VERSION"),
        "ci tokenizer",
        &seed,
        None,
        spec,
        &harness_command,
        run,
    );

    fs::write(&options.report_out, report.to_json())?;

    if status == HarnessStatus::Failed {
        return Err(CorpusForgeError::predicate_failure(
            "tokenizer harness failed; see `--report-out` for the first failing sample",
        ));
    }

    Ok(format!(
        "tokenizer ci passed\nseed: {seed}\nunicode_mode: {mode}\noutput_kind: {output_kind}\ncase_count: {case_count}\nreport_out: {report_out}",
        mode = options.mode,
        output_kind = options.output_kind,
        case_count = options.case_count,
        report_out = options.report_out.display()
    ))
}

fn execute_gen(options: GenOptions, stdout: &mut impl std::io::Write) -> Result<Option<String>> {
    match options {
        GenOptions::Profile(options) => execute_profile_gen(options, stdout),
        GenOptions::Unicode(options) => execute_unicode_gen(options, stdout),
    }
}

fn execute_profile_gen(
    options: ProfileGenOptions,
    stdout: &mut impl std::io::Write,
) -> Result<Option<String>> {
    let seed = read_seed(&options.seed_source)?;
    let profile_bytes = fs::read(&options.profile)?;
    let pack = ProfilePack::from_bytes(&profile_bytes)?;
    let profile_hash = pack.profile_hash();
    let model_bytes = pack.ngram_model_bytes().ok_or_else(|| {
        CorpusForgeError::invalid_profile(
            "profile lacks required NGRAMV0\\0 n-gram model section; rebuild it with `corpusforge profile build <input> --out <path>`",
        )
    })?;
    let model = ByteBigramModel::from_bytes(model_bytes)?;

    if let Some(out) = &options.out {
        let mut file = File::create(out)?;
        model.generate_bytes(&seed, options.byte_count, &mut file)?;
        file.flush()?;
    } else {
        model.generate_bytes(&seed, options.byte_count, stdout)?;
    }

    if let Some(metadata_out) = &options.metadata_out {
        fs::write(
            metadata_out,
            format_gen_metadata(&options, &seed, &profile_hash),
        )?;
    }

    if options.out.is_none() || options.quiet {
        return Ok(None);
    }

    Ok(Some(format_gen_summary(&options, &seed, &profile_hash)))
}

fn execute_unicode_gen(
    options: UnicodeGenOptions,
    stdout: &mut impl std::io::Write,
) -> Result<Option<String>> {
    let seed = read_seed(&options.seed_source)?;
    let spec = TokenizerCaseSpec::new(options.mode, options.output_kind, options.case_count)?;
    let cases = generate_tokenizer_cases(&seed, spec)?;
    let bytes = join_tokenizer_cases(&cases);

    if let Some(out) = &options.out {
        let mut file = File::create(out)?;
        file.write_all(&bytes)?;
        file.flush()?;
    } else {
        stdout.write_all(&bytes)?;
    }

    if options.out.is_none() || options.quiet {
        return Ok(None);
    }

    Ok(Some(format_unicode_gen_summary(
        &options,
        &seed,
        bytes.len(),
    )))
}

fn join_tokenizer_cases(cases: &[corpusforge_tokenizer::TokenizerCase]) -> Vec<u8> {
    let mut bytes = Vec::new();

    for (index, case) in cases.iter().enumerate() {
        if index > 0 {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(case.bytes());
    }

    bytes
}

fn read_seed(seed_source: &SeedSource) -> Result<MasterSeed> {
    match seed_source {
        SeedSource::Inline(text) => MasterSeed::from_str(text),
        SeedSource::File(path) => MasterSeed::from_seed_file(path),
    }
}

fn format_gen_summary(
    options: &ProfileGenOptions,
    seed: &MasterSeed,
    profile_hash: &str,
) -> String {
    let out = options
        .out
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "stdout".to_owned());

    if options.json {
        return format!(
            "{{\"command\":\"gen\",\"seed\":\"{}\",\"profile_hash\":\"{}\",\"engine_name\":\"{}\",\"engine_version\":{},\"byte_count\":{},\"out\":\"{}\"}}",
            seed,
            json_escape(profile_hash),
            json_escape(ENGINE_NAME),
            ENGINE_VERSION,
            options.byte_count,
            json_escape(&out)
        );
    }

    format!(
        "generated corpus\nprofile_hash: {profile_hash}\nseed: {seed}\nengine: {engine_name}/{engine_version}\nbyte_count: {byte_count}\nout: {out}",
        engine_name = ENGINE_NAME,
        engine_version = ENGINE_VERSION,
        byte_count = options.byte_count
    )
}

fn format_unicode_gen_summary(
    options: &UnicodeGenOptions,
    seed: &MasterSeed,
    byte_count: usize,
) -> String {
    let out = options
        .out
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "stdout".to_owned());

    format!(
        "generated unicode corpus\nseed: {seed}\nunicode_mode: {mode}\noutput_kind: {output_kind}\ncase_count: {case_count}\nbyte_count: {byte_count}\nout: {out}",
        mode = options.mode,
        output_kind = options.output_kind,
        case_count = options.case_count
    )
}

fn format_gen_metadata(
    options: &ProfileGenOptions,
    seed: &MasterSeed,
    profile_hash: &str,
) -> String {
    let output_mode = if options.out.is_some() {
        "file"
    } else {
        "stdout"
    };
    format!(
        "{{\"tool_version\":\"{}\",\"command\":\"gen\",\"seed\":\"{}\",\"profile_hash\":\"{}\",\"engine_name\":\"{}\",\"engine_version\":{},\"byte_count\":{},\"determinism\":\"{}\",\"output_mode\":\"{}\",\"json_summary\":{},\"quiet\":{}}}\n",
        json_escape(env!("CARGO_PKG_VERSION")),
        seed,
        json_escape(profile_hash),
        json_escape(ENGINE_NAME),
        ENGINE_VERSION,
        options.byte_count,
        options.determinism.as_str(),
        output_mode,
        options.json,
        options.quiet
    )
}

fn execute_profile_command(command: ProfileCommand) -> Result<String> {
    match command {
        ProfileCommand::Build(options) => execute_profile_build(options),
        ProfileCommand::Inspect(options) => execute_profile_inspect(options),
        ProfileCommand::Verify(options) => execute_profile_verify(options),
    }
}

fn execute_profile_build(options: ProfileBuildOptions) -> Result<String> {
    let pack = compile_path(&options.input)?;
    let bytes = pack.to_bytes();
    fs::write(&options.out, bytes)?;

    format_profile_summary("built profile", &pack.inspect(), options.json)
}

fn execute_profile_inspect(options: ProfileFileOptions) -> Result<String> {
    let bytes = fs::read(&options.profile)?;
    let summary = ProfilePack::verify_bytes(&bytes)?;

    format_profile_summary("inspected profile", &summary, options.json)
}

fn execute_profile_verify(options: ProfileFileOptions) -> Result<String> {
    let bytes = fs::read(&options.profile)?;
    let summary = ProfilePack::verify_bytes(&bytes)?;

    format_profile_summary("verified profile", &summary, options.json)
}

fn format_profile_summary(action: &str, summary: &InspectSummary, json: bool) -> Result<String> {
    if json {
        return Ok(format!(
            "{{\"version\":{},\"profile_hash\":\"{}\",\"file_count\":{},\"byte_count\":{}}}",
            summary.version,
            json_escape(&summary.profile_hash),
            summary.file_count,
            summary.total_file_bytes
        ));
    }

    Ok(format!(
        "{action}\nversion: {version}\nprofile_hash: {profile_hash}\nfile_count: {file_count}\nbyte_count: {byte_count}",
        version = summary.version,
        profile_hash = summary.profile_hash,
        file_count = summary.file_count,
        byte_count = summary.total_file_bytes
    ))
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();

    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }

    escaped
}

fn parse_common_options(command: &CommandSpec, args: &[OsString]) -> Result<()> {
    let mut state = CommonOptionState::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();

        if let Some(option) = common_value_option(flag.as_ref()) {
            option.claim(&mut state)?;
            let value = take_value(command, args, index, option.flag())?;
            option.validate(&value)?;
            index += 2;
            continue;
        }

        match flag.as_ref() {
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

enum CommonValueOption {
    Seed {
        flag: &'static str,
    },
    Once {
        field: &'static str,
        flag: &'static str,
        validation: CommonValueValidation,
    },
}

impl CommonValueOption {
    fn flag(&self) -> &'static str {
        match self {
            Self::Seed { flag } | Self::Once { flag, .. } => flag,
        }
    }

    fn claim(&self, state: &mut CommonOptionState) -> Result<()> {
        match self {
            Self::Seed { flag } => state.claim_seed(flag),
            Self::Once { field, flag, .. } => state.claim_once(field, flag),
        }
    }

    fn validate(&self, value: &str) -> Result<()> {
        match self {
            Self::Seed { flag } => validate_non_empty(value, flag),
            Self::Once {
                flag, validation, ..
            } => validation.validate(value, flag),
        }
    }
}

enum CommonValueValidation {
    NonEmpty,
    ByteSize,
    Determinism,
}

impl CommonValueValidation {
    fn validate(&self, value: &str, flag: &str) -> Result<()> {
        match self {
            Self::NonEmpty => validate_non_empty(value, flag),
            Self::ByteSize => parse_byte_size(value).map(|_| ()),
            Self::Determinism => parse_determinism(value),
        }
    }
}

fn common_value_option(flag: &str) -> Option<CommonValueOption> {
    match flag {
        "--seed" => Some(CommonValueOption::Seed { flag: "--seed" }),
        "--seed-file" => Some(CommonValueOption::Seed {
            flag: "--seed-file",
        }),
        "--profile" => Some(CommonValueOption::Once {
            field: "profile",
            flag: "--profile",
            validation: CommonValueValidation::NonEmpty,
        }),
        "--out" => Some(CommonValueOption::Once {
            field: "out",
            flag: "--out",
            validation: CommonValueValidation::NonEmpty,
        }),
        "--bytes" => Some(CommonValueOption::Once {
            field: "bytes",
            flag: "--bytes",
            validation: CommonValueValidation::ByteSize,
        }),
        "--determinism" => Some(CommonValueOption::Once {
            field: "determinism",
            flag: "--determinism",
            validation: CommonValueValidation::Determinism,
        }),
        "--metadata-out" => Some(CommonValueOption::Once {
            field: "metadata_out",
            flag: "--metadata-out",
            validation: CommonValueValidation::NonEmpty,
        }),
        _ => None,
    }
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
    parse_determinism_mode(value).map(|_| ())
}

fn parse_determinism_mode(value: &str) -> Result<DeterminismMode> {
    match value {
        "strict" => Ok(DeterminismMode::Strict),
        "relaxed" => Ok(DeterminismMode::Relaxed),
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

fn parse_case_count(value: &str) -> Result<usize> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CorpusForgeError::invalid_argument(format!(
            "invalid case count `{value}`; expected a positive integer"
        )));
    }

    let parsed = value.parse::<usize>().map_err(|_| {
        CorpusForgeError::invalid_argument(format!(
            "invalid case count `{value}`; expected a positive integer"
        ))
    })?;

    if parsed == 0 {
        return Err(CorpusForgeError::invalid_argument(
            "case count must be greater than zero",
        ));
    }

    Ok(parsed)
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
    if command.name == "profile" {
        return profile_help(command);
    }

    if command.name == "ci" {
        return ci_help(command);
    }

    if command.name == "gen" {
        return gen_help(command);
    }

    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge {name} [OPTIONS]\n\nOPTIONS:\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a file\n    --profile <path>              Read a CorpusForge profile path\n    --out <path>                  Write generated output to a path\n    --bytes <N>                   Set output size in bytes; supports KB, MB, GB\n    --determinism <mode>          Determinism mode: strict or relaxed\n    --metadata-out <path>         Write machine-readable metadata to a path\n    --quiet                       Reduce human-readable output\n    --json                        Emit machine-readable JSON where supported\n    -h, --help                    Print help\n\nEXAMPLES:\n    corpusforge {name} --seed 42 --profile profiles/smoke.cff --bytes 64KB\n    corpusforge {name} --seed-file seed.txt --determinism strict --metadata-out report.json --json\n\nSTATUS:\n    Planned for a later milestone; execution currently returns NotImplemented.",
        name = command.name,
        summary = command.summary
    )
}

fn ci_help(command: &CommandSpec) -> String {
    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge ci tokenizer --unicode <mode> --output-kind <valid-text|raw-bytes> --cases <N> (--seed <seed> | --seed-file <path>) --command <path> [--arg <value> ...] --report-out <path>\n    corpusforge ci [OPTIONS]\n\nSUBCOMMANDS:\n    tokenizer    Run an external tokenizer harness once per generated Unicode sample\n\nOPTIONS:\n    --unicode <mode>              Generate built-in tokenizer Unicode stress cases\n    --output-kind <kind>          Unicode output boundary: valid-text or raw-bytes\n    --cases <N>                   Number of Unicode tokenizer cases to generate\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a 32-byte file\n    --command <path>              Harness executable path; invoked directly without a shell\n    --arg <value>                 Literal harness argument; may be repeated and preserves order\n    --report-out <path>           Write stable tokenizer report JSON to a path\n    --profile <path>              Placeholder profile path option for later CI checks\n    --out <path>                  Placeholder output path option for later CI checks\n    --bytes <N>                   Placeholder output size in bytes; supports KB, MB, GB\n    --determinism <mode>          Placeholder determinism mode: strict or relaxed\n    --metadata-out <path>         Placeholder machine-readable metadata path\n    --quiet                       Placeholder quiet mode for later CI checks\n    --json                        Placeholder machine-readable output mode\n    -h, --help                    Print help\n\nUNICODE MODES:\n    grapheme, bidi, zero-width, emoji, normalization, mixed, invalid-utf8\n\nREPORT:\n    Writes TokenizerReport JSON on both pass and fail. The command field is `ci tokenizer`, and profile_hash is null.",
        name = command.name,
        summary = command.summary
    )
}

fn gen_help(command: &CommandSpec) -> String {
    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge gen --profile <path> (--seed <seed> | --seed-file <path>) --bytes <N> [OPTIONS]\n    corpusforge gen --unicode <mode> --output-kind <valid-text|raw-bytes> --cases <N> (--seed <seed> | --seed-file <path>) [--out <path>] [--quiet]\n\nOPTIONS:\n    --profile <path>              Read a CorpusForge .cff profile with an embedded n-gram model\n    --unicode <mode>              Generate built-in tokenizer Unicode stress cases\n    --output-kind <kind>          Unicode output boundary: valid-text or raw-bytes\n    --cases <N>                   Number of Unicode tokenizer cases to generate\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a 32-byte file\n    --bytes <N>                   Set profile-backed output size in bytes; supports KB, MB, GB\n    --out <path>                  Stream generated bytes to a file instead of stdout\n    --determinism <mode>          Profile-backed determinism mode: strict or relaxed\n    --metadata-out <path>         Write profile-backed stable generation metadata JSON to a path\n    --quiet                       Suppress human-readable output when --out is used\n    --json                        Emit profile-backed JSON summary when --out is used\n    -h, --help                    Print help\n\nUNICODE MODES:\n    grapheme, bidi, zero-width, emoji, normalization, mixed, invalid-utf8\n\nSTDOUT:\n    Without --out, generated binary bytes are written directly to stdout without an added trailing newline. Use --out before --json.",
        name = command.name,
        summary = command.summary
    )
}

fn profile_help(command: &CommandSpec) -> String {
    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge profile build <input> --out <path> [--json]\n    corpusforge profile inspect --profile <path> [--json]\n    corpusforge profile verify --profile <path> [--json]\n\nSUBCOMMANDS:\n    build      Compile a fixture file or directory into a .cff profile\n    inspect    Read a .cff profile and print deterministic summary metadata\n    verify     Verify a .cff profile envelope and print deterministic summary metadata\n\nOPTIONS:\n    --profile <path>              Read a CorpusForge .cff profile path\n    --out <path>                  Write the compiled .cff profile to a path\n    --json                        Emit stable machine-readable JSON\n    -h, --help                    Print help\n\nALIASES:\n    corpusforge verify --profile <path>\n\nUNSUPPORTED:\n    Future flags such as --format and --unicode are rejected until implemented.",
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
    use std::io::{self, Read};

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
            assert!(help.contains("--profile <path>"));
            if command == "profile" {
                assert!(help.contains("build <input> --out <path>"));
                assert!(help.contains("inspect --profile <path>"));
                assert!(help.contains("verify --profile <path>"));
                assert!(help.contains("corpusforge verify --profile <path>"));
            } else if command == "ci" {
                assert!(help.contains("corpusforge ci tokenizer"));
                assert!(help.contains("--unicode <mode>"));
                assert!(help.contains("--output-kind <kind>"));
                assert!(help.contains("--cases <N>"));
                assert!(help.contains("--command <path>"));
                assert!(help.contains("--arg <value>"));
                assert!(help.contains("--report-out <path>"));
                assert!(help.contains("TokenizerReport"));
                assert!(!help.contains("Planned for a later milestone"));
            } else {
                assert!(help.contains("--seed <seed>"));
                assert!(help.contains("--seed-file <path>"));
                assert!(help.contains("--out <path>"));
                assert!(help.contains("--bytes <N>"));
                assert!(help.contains("--determinism <mode>"));
                assert!(help.contains("--metadata-out <path>"));
                assert!(help.contains("--quiet"));
                assert!(help.contains("--json"));
                if command == "gen" {
                    assert!(help.contains("generated binary bytes"));
                    assert!(help.contains("--unicode <mode>"));
                    assert!(help.contains("--output-kind <kind>"));
                    assert!(help.contains("--cases <N>"));
                    assert!(help.contains("invalid-utf8"));
                    assert!(!help.contains("Planned for a later milestone"));
                } else {
                    assert!(help.contains("EXAMPLES"));
                    assert!(help.contains("Planned for a later milestone"));
                }
            }
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
        assert!(help.contains("--unicode <mode>"));
        assert!(help.contains("generated binary bytes"));
        assert!(!help.contains("Planned for a later milestone"));
    }

    #[test]
    fn placeholder_command_returns_not_implemented() {
        let CliOutcome::Failure(error) = run(["corpusforge", "shrink"]) else {
            panic!("placeholder execution should fail");
        };

        assert_eq!(error.category(), "not_implemented");
        assert!(error.to_string().contains("shrink command execution"));
    }

    #[test]
    fn common_options_parse_before_placeholder_execution() {
        let CliOutcome::Failure(error) = run([
            "corpusforge",
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
        ]) else {
            panic!("placeholder execution should fail");
        };

        assert_eq!(error.category(), "not_implemented");
        assert!(error.to_string().contains("replay command execution"));
    }

    #[test]
    fn ci_tokenizer_requires_all_options() {
        let cases = [
            (
                &["corpusforge", "ci", "tokenizer", "--cases", "1"][..],
                "missing required option `--unicode`",
            ),
            (
                &["corpusforge", "ci", "tokenizer", "--unicode", "mixed"][..],
                "missing required option `--output-kind`",
            ),
            (
                &[
                    "corpusforge",
                    "ci",
                    "tokenizer",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                ][..],
                "missing required option `--cases`",
            ),
            (
                &[
                    "corpusforge",
                    "ci",
                    "tokenizer",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "1",
                ][..],
                "missing required seed source",
            ),
            (
                &[
                    "corpusforge",
                    "ci",
                    "tokenizer",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "1",
                    "--seed",
                    "1337",
                ][..],
                "missing required option `--command`",
            ),
            (
                &[
                    "corpusforge",
                    "ci",
                    "tokenizer",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "1",
                    "--seed",
                    "1337",
                    "--command",
                    "tokenizer-harness",
                ][..],
                "missing required option `--report-out`",
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
    fn ci_tokenizer_pass_writes_stable_report_and_preserves_arg_order() {
        let report = temp_report_path("ci-tokenizer-pass");
        let helper = std::env::current_exe().expect("test executable should exist");
        let helper_text = helper.display().to_string();
        let report_text = report.display().to_string();

        let outcome = run([
            "corpusforge",
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
            &helper_text,
            "--arg",
            "--ignored",
            "--arg",
            "--exact",
            "--arg",
            "tests::ci_harness_accepts_nonempty_input",
            "--report-out",
            &report_text,
        ]);

        let CliOutcome::Success(summary) = outcome else {
            panic!("ci tokenizer should pass");
        };

        assert!(summary.contains("tokenizer ci passed"));
        let json = std::fs::read_to_string(&report).expect("report should be written");
        assert!(json.contains("\"command\":\"ci tokenizer\""));
        assert!(json.contains("\"profile_hash\":null"));
        assert!(json.contains("\"unicode_mode\":\"grapheme\""));
        assert!(json.contains("\"status\":\"passed\""));
        assert!(json.contains("\"failure_sample\":null"));
        assert!(json.contains(&format!(
            "\"harness_command\":\"{} --ignored --exact tests::ci_harness_accepts_nonempty_input\"",
            json_escape_for_test(&helper_text)
        )));

        let _ = std::fs::remove_file(report);
    }

    #[test]
    fn ci_tokenizer_failure_writes_report_before_nonzero_outcome() {
        let report = temp_report_path("ci-tokenizer-fail");
        let helper = std::env::current_exe().expect("test executable should exist");
        let helper_text = helper.display().to_string();
        let report_text = report.display().to_string();

        let outcome = run([
            "corpusforge",
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
            &helper_text,
            "--arg",
            "--ignored",
            "--arg",
            "--exact",
            "--arg",
            "tests::ci_harness_rejects_all_input",
            "--report-out",
            &report_text,
        ]);

        let CliOutcome::Failure(error) = outcome else {
            panic!("ci tokenizer should fail when harness fails");
        };

        assert_eq!(error.category(), "predicate_failure");
        let json = std::fs::read_to_string(&report).expect("report should be written");
        assert!(json.contains("\"status\":\"failed\""));
        assert!(json.contains("\"failure_sample\":{\"case_index\":0"));
        assert!(json.contains("\"exit_code\":101"));

        let _ = std::fs::remove_file(report);
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
    fn gen_requires_profile_seed_and_bytes() {
        let cases = [
            (
                &["corpusforge", "gen", "--seed", "42", "--bytes", "1024"][..],
                "missing required option `--profile`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--profile",
                    "profiles/smoke.cff",
                    "--bytes",
                    "1024",
                ][..],
                "missing required seed source",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--profile",
                    "profiles/smoke.cff",
                    "--seed",
                    "42",
                ][..],
                "missing required option `--bytes`",
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
    fn gen_unicode_requires_seed_output_kind_and_cases() {
        let cases = [
            (
                &["corpusforge", "gen", "--unicode", "mixed", "--cases", "12"][..],
                "missing required option `--output-kind`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                ][..],
                "missing required option `--cases`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "12",
                ][..],
                "missing required seed source",
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
    fn gen_unicode_rejects_profile_backed_options() {
        let cases = [
            (
                &[
                    "corpusforge",
                    "gen",
                    "--profile",
                    "profiles/smoke.cff",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "12",
                    "--seed",
                    "1337",
                ][..],
                "cannot be mixed",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "12",
                    "--seed",
                    "1337",
                    "--json",
                ][..],
                "only supported for profile-backed",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "12",
                    "--seed",
                    "1337",
                    "--metadata-out",
                    "metadata.json",
                ][..],
                "only supported for profile-backed",
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
    fn gen_unicode_valid_text_stdout_is_deterministic() {
        let args = [
            "corpusforge",
            "gen",
            "--unicode",
            "mixed",
            "--output-kind",
            "valid-text",
            "--cases",
            "12",
            "--seed",
            "1337",
        ];
        let CliOutcome::SuccessBytes(first) = run(args) else {
            panic!("unicode generation should write bytes");
        };
        let CliOutcome::SuccessBytes(second) = run(args) else {
            panic!("unicode generation should write bytes");
        };

        assert_eq!(first, second);
        assert_eq!(
            bytes_to_hex(&first),
            fixture("seed_1337_unicode_valid_text_mixed.hex")
        );
        assert!(std::str::from_utf8(&first).is_ok());
        assert_ne!(first.last(), Some(&b'\n'));
    }

    #[test]
    fn gen_unicode_raw_bytes_can_emit_invalid_utf8() {
        let CliOutcome::SuccessBytes(bytes) = run([
            "corpusforge",
            "gen",
            "--unicode",
            "invalid-utf8",
            "--output-kind",
            "raw-bytes",
            "--cases",
            "12",
            "--seed",
            "1337",
        ]) else {
            panic!("unicode generation should write bytes");
        };

        assert_eq!(
            bytes_to_hex(&bytes),
            fixture("seed_1337_unicode_raw_bytes_invalid_utf8.hex")
        );
        assert!(std::str::from_utf8(&bytes).is_err());
        assert_ne!(bytes.last(), Some(&b'\n'));
    }

    #[test]
    fn gen_rejects_json_without_out() {
        let CliOutcome::Failure(error) = run([
            "corpusforge",
            "gen",
            "--profile",
            "profiles/smoke.cff",
            "--seed",
            "42",
            "--bytes",
            "1024",
            "--json",
        ]) else {
            panic!("json without out should fail");
        };

        assert_eq!(error.category(), "invalid_argument");
        assert!(error
            .to_string()
            .contains("standard output carries generated binary bytes"));
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
            (
                &["corpusforge", "gen", "--unicode", "unknown"][..],
                "unsupported Unicode mode",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "stream",
                ][..],
                "unsupported Unicode output kind",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--unicode",
                    "mixed",
                    "--output-kind",
                    "valid-text",
                    "--cases",
                    "0",
                ][..],
                "case count must be greater than zero",
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

    #[test]
    #[ignore]
    fn ci_harness_accepts_nonempty_input() {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .expect("helper should read stdin");

        assert!(!input.is_empty());
    }

    #[test]
    #[ignore]
    fn ci_harness_rejects_all_input() {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .expect("helper should read stdin");

        assert!(input.is_empty());
    }

    fn temp_report_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("corpusforge-cli-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temporary report directory should be writable");

        dir.join(format!("corpusforge-cli-{name}.json"))
    }

    fn json_escape_for_test(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn bytes_to_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }

    fn fixture(name: &str) -> &'static str {
        match name {
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
}
