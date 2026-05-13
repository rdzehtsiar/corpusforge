// SPDX-License-Identifier: Apache-2.0

//! Command-line parsing and execution for the CorpusForge binary.

use corpusforge_cff::{InspectSummary, ProfilePack};
use corpusforge_core::output::ByteRangeWriter;
use corpusforge_core::seed::MasterSeed;
use corpusforge_core::{CorpusForgeError, Result};
use corpusforge_grammar::{
    generate_grammar_cases, GrammarCase, GrammarCaseSpec, GrammarFormat, GrammarMode,
};
use corpusforge_ngram::{ByteBigramModel, ENGINE_NAME, ENGINE_VERSION};
use corpusforge_profile::compile_path;
use corpusforge_shrink::{
    shrink_bytes, PredicateCommand, PredicateFailureKind, ShrinkConfig, ShrinkOutcome,
    DEFAULT_MAX_RUNS, DEFAULT_TIMEOUT_MS,
};
use corpusforge_tokenizer::{
    generate_tokenizer_cases, run_stdin_harness, HarnessCommand, HarnessStatus, TokenizerCaseSpec,
    TokenizerReport, UnicodeMode, UnicodeOutputKind,
};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
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
    ExecuteReplay(ReplayOptions),
    ExecuteShrink(ShrinkOptions),
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
    Grammar(GrammarGenOptions),
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
struct GrammarGenOptions {
    format: GrammarFormat,
    mode: GrammarMode,
    unicode_mode: Option<UnicodeMode>,
    case_count: usize,
    seed_source: SeedSource,
    out: Option<PathBuf>,
    quiet: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct ReplayOptions {
    profile: PathBuf,
    seed_source: SeedSource,
    range: ByteRange,
    out: Option<PathBuf>,
    metadata_out: Option<PathBuf>,
    quiet: bool,
    json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    const fn byte_count(self) -> u64 {
        self.end - self.start
    }
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
struct ShrinkOptions {
    input: PathBuf,
    predicate: PathBuf,
    predicate_args: Vec<String>,
    out: PathBuf,
    metadata_out: Option<PathBuf>,
    timeout_ms: u64,
    max_runs: usize,
    quiet: bool,
    json: bool,
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
pub fn run_to_writers<I, S>(args: I, stdout: &mut impl Write, stderr: &mut impl Write) -> i32
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
pub fn write_outcome(outcome: CliOutcome, stdout: &mut impl Write, stderr: &mut impl Write) -> i32 {
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
        command_name => parse_command(command_name, args.collect::<Vec<_>>()),
    }
}

fn parse_command(command_name: &str, remaining: Vec<OsString>) -> Result<ParsedCommand> {
    let command = find_command(command_name).ok_or_else(|| {
        CorpusForgeError::invalid_profile(format!(
            "unknown command `{command_name}`; run `corpusforge --help`"
        ))
    })?;

    match command.name {
        "ci" if first_arg_is(&remaining, "tokenizer") => {
            parse_ci_tokenizer_command(command, &remaining)
        }
        "shrink" => parse_shrink_command(command, &remaining),
        _ if contains_help_flag(&remaining) => Ok(ParsedCommand::CommandHelp(command)),
        "profile" => parse_profile_command(&remaining).map(ParsedCommand::ExecuteProfile),
        "gen" => parse_gen_options(&remaining).map(ParsedCommand::ExecuteGen),
        "replay" => parse_replay_options(&remaining).map(ParsedCommand::ExecuteReplay),
        "verify" if contains_profile_option(&remaining) => {
            parse_profile_file_options("verify", &remaining).map(ParsedCommand::ExecuteVerifyAlias)
        }
        _ => {
            parse_common_options(command, &remaining)?;
            Ok(ParsedCommand::ExecutePlaceholder(command))
        }
    }
}

fn parse_ci_tokenizer_command(
    command: &'static CommandSpec,
    remaining: &[OsString],
) -> Result<ParsedCommand> {
    let tokenizer_args = &remaining[1..];
    if contains_only_help_flag(tokenizer_args) {
        Ok(ParsedCommand::CommandHelp(command))
    } else {
        parse_ci_tokenizer_options(tokenizer_args).map(ParsedCommand::ExecuteCiTokenizer)
    }
}

fn parse_shrink_command(
    command: &'static CommandSpec,
    remaining: &[OsString],
) -> Result<ParsedCommand> {
    if contains_only_help_flag(remaining) {
        Ok(ParsedCommand::CommandHelp(command))
    } else {
        parse_shrink_options(remaining).map(ParsedCommand::ExecuteShrink)
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
    let mut state = GenOptionState::new();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();
        index += parse_gen_option(command, args, index, flag.as_ref(), &mut state)?;
    }

    finish_gen_options(state)
}

struct GenOptionState {
    profile: Option<PathBuf>,
    seed_source: Option<SeedSource>,
    byte_count: Option<usize>,
    unicode_mode: Option<UnicodeMode>,
    output_kind: Option<UnicodeOutputKind>,
    grammar_format: Option<GrammarFormat>,
    grammar_mode: Option<GrammarMode>,
    case_count: Option<usize>,
    out: Option<PathBuf>,
    metadata_out: Option<PathBuf>,
    determinism: DeterminismMode,
    determinism_set: bool,
    quiet: bool,
    json: bool,
}

impl GenOptionState {
    const fn new() -> Self {
        Self {
            profile: None,
            seed_source: None,
            byte_count: None,
            unicode_mode: None,
            output_kind: None,
            grammar_format: None,
            grammar_mode: None,
            case_count: None,
            out: None,
            metadata_out: None,
            determinism: DeterminismMode::Strict,
            determinism_set: false,
            quiet: false,
            json: false,
        }
    }
}

fn parse_gen_option(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    flag: &str,
    state: &mut GenOptionState,
) -> Result<usize> {
    match flag {
        "--profile" | "--seed" | "--seed-file" | "--bytes" | "--unicode" | "--output-kind"
        | "--grammar" | "--grammar-mode" | "--cases" | "--out" | "--metadata-out"
        | "--determinism" => parse_gen_value_option(command, args, index, flag, state),
        "--quiet" | "--json" => parse_gen_switch_option(flag, state),
        "-h" | "--help" => Err(CorpusForgeError::invalid_argument(
            "help must be requested without other arguments; run `corpusforge gen --help`",
        )),
        other if other.starts_with('-') => Err(CorpusForgeError::invalid_argument(format!(
            "unknown option `{other}` for `gen`; run `corpusforge gen --help`"
        ))),
        other => Err(CorpusForgeError::invalid_argument(format!(
            "unexpected argument `{other}` for `gen`; run `corpusforge gen --help`"
        ))),
    }
}

fn parse_gen_value_option(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    flag: &str,
    state: &mut GenOptionState,
) -> Result<usize> {
    match flag {
        "--profile" => {
            state.profile = Some(take_unique_path(&state.profile, "gen", args, index, flag)?)
        }
        "--seed" => {
            state.seed_source = Some(take_inline_seed(
                command,
                args,
                index,
                flag,
                &state.seed_source,
            )?)
        }
        "--seed-file" => {
            state.seed_source = Some(take_file_seed(
                "gen",
                args,
                index,
                flag,
                &state.seed_source,
            )?)
        }
        "--bytes" => {
            state.byte_count = Some(take_byte_count(command, args, index, &state.byte_count)?)
        }
        "--unicode" => {
            state.unicode_mode = Some(take_unicode_mode(
                command,
                args,
                index,
                &state.unicode_mode,
            )?)
        }
        "--output-kind" => {
            state.output_kind = Some(take_output_kind(command, args, index, &state.output_kind)?)
        }
        "--grammar" => {
            state.grammar_format = Some(take_grammar_format(
                command,
                args,
                index,
                &state.grammar_format,
            )?)
        }
        "--grammar-mode" => {
            state.grammar_mode = Some(take_grammar_mode(
                command,
                args,
                index,
                &state.grammar_mode,
            )?)
        }
        "--cases" => {
            state.case_count = Some(take_case_count(command, args, index, &state.case_count)?)
        }
        "--out" => state.out = Some(take_unique_path(&state.out, "gen", args, index, flag)?),
        "--metadata-out" => {
            state.metadata_out = Some(take_unique_path(
                &state.metadata_out,
                "gen",
                args,
                index,
                flag,
            )?)
        }
        "--determinism" => set_gen_determinism(command, args, index, state)?,
        _ => unreachable!("gen value option should be prefiltered"),
    }
    Ok(2)
}

fn parse_gen_switch_option(flag: &str, state: &mut GenOptionState) -> Result<usize> {
    match flag {
        "--quiet" => claim_switch(&mut state.quiet, flag)?,
        "--json" => claim_switch(&mut state.json, flag)?,
        _ => unreachable!("gen switch option should be prefiltered"),
    }
    Ok(1)
}

fn finish_gen_options(state: GenOptionState) -> Result<GenOptions> {
    let uses_profile_path = state.profile.is_some() || state.byte_count.is_some();
    let uses_grammar_path = state.grammar_format.is_some() || state.grammar_mode.is_some();
    let uses_unicode_only_path = state.output_kind.is_some()
        || (state.unicode_mode.is_some() && !uses_grammar_path)
        || (state.case_count.is_some() && !uses_grammar_path);

    if uses_profile_path && (uses_unicode_only_path || uses_grammar_path) {
        return Err(CorpusForgeError::invalid_argument(
            "`gen` profile/bytes options cannot be mixed with Unicode-only or grammar generation options",
        ));
    }

    if uses_grammar_path && state.output_kind.is_some() {
        return Err(CorpusForgeError::invalid_argument(
            "`gen` grammar options cannot be mixed with Unicode-only `--output-kind`",
        ));
    }

    if uses_grammar_path {
        return finish_grammar_gen_options(state);
    }

    if uses_unicode_only_path {
        return finish_unicode_gen_options(state);
    }

    finish_profile_gen_options(state)
}

fn finish_unicode_gen_options(state: GenOptionState) -> Result<GenOptions> {
    reject_profile_only_gen_options(
        state.json,
        state.metadata_out.as_ref(),
        state.determinism_set,
    )?;

    let mode = state.unicode_mode.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--unicode` for `gen`")
    })?;
    let output_kind = state.output_kind.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--output-kind` for `gen`")
    })?;
    let case_count = state.case_count.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--cases` for `gen`")
    })?;
    let seed_source = require_seed_source(state.seed_source, "gen")?;

    TokenizerCaseSpec::new(mode, output_kind, case_count)?;

    Ok(GenOptions::Unicode(UnicodeGenOptions {
        mode,
        output_kind,
        case_count,
        seed_source,
        out: state.out,
        quiet: state.quiet,
    }))
}

fn finish_grammar_gen_options(state: GenOptionState) -> Result<GenOptions> {
    reject_profile_only_gen_options(
        state.json,
        state.metadata_out.as_ref(),
        state.determinism_set,
    )?;

    let format = state.grammar_format.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--grammar` for `gen`")
    })?;
    let mode = state.grammar_mode.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--grammar-mode` for `gen`")
    })?;
    let case_count = state.case_count.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--cases` for `gen`")
    })?;
    let seed_source = require_seed_source(state.seed_source, "gen")?;

    GrammarCaseSpec::new(format, mode, case_count, state.unicode_mode)?;

    Ok(GenOptions::Grammar(GrammarGenOptions {
        format,
        mode,
        unicode_mode: state.unicode_mode,
        case_count,
        seed_source,
        out: state.out,
        quiet: state.quiet,
    }))
}

fn finish_profile_gen_options(state: GenOptionState) -> Result<GenOptions> {
    if state.json && state.out.is_none() {
        return Err(CorpusForgeError::invalid_argument(
            "`--json` requires `--out` for `gen` because standard output carries generated binary bytes",
        ));
    }

    let profile = state.profile.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--profile` for `gen`")
    })?;
    let seed_source = require_seed_source(state.seed_source, "gen")?;
    let byte_count = state.byte_count.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--bytes` for `gen`")
    })?;

    Ok(GenOptions::Profile(ProfileGenOptions {
        profile,
        seed_source,
        byte_count,
        out: state.out,
        metadata_out: state.metadata_out,
        determinism: state.determinism,
        quiet: state.quiet,
        json: state.json,
    }))
}

fn parse_replay_options(args: &[OsString]) -> Result<ReplayOptions> {
    let command = find_command("replay").expect("replay command should exist");
    let mut state = ReplayOptionState::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();
        index += parse_replay_option(command, args, index, flag.as_ref(), &mut state)?;
    }

    finish_replay_options(state)
}

#[derive(Default)]
struct ReplayOptionState {
    profile: Option<PathBuf>,
    seed_source: Option<SeedSource>,
    range: Option<ByteRange>,
    out: Option<PathBuf>,
    metadata_out: Option<PathBuf>,
    quiet: bool,
    json: bool,
}

fn parse_replay_option(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    flag: &str,
    state: &mut ReplayOptionState,
) -> Result<usize> {
    match flag {
        "--profile" => {
            state.profile = Some(take_unique_path(
                &state.profile,
                "replay",
                args,
                index,
                flag,
            )?)
        }
        "--seed" => {
            state.seed_source = Some(take_inline_seed(
                command,
                args,
                index,
                flag,
                &state.seed_source,
            )?)
        }
        "--seed-file" => {
            state.seed_source = Some(take_file_seed(
                "replay",
                args,
                index,
                flag,
                &state.seed_source,
            )?)
        }
        "--range" => state.range = Some(take_replay_range(command, args, index, &state.range)?),
        "--out" => state.out = Some(take_unique_path(&state.out, "replay", args, index, flag)?),
        "--metadata-out" => {
            state.metadata_out = Some(take_unique_path(
                &state.metadata_out,
                "replay",
                args,
                index,
                flag,
            )?)
        }
        _ => return parse_quiet_json_or_reject("replay", flag, &mut state.quiet, &mut state.json),
    }

    Ok(2)
}

fn take_replay_range(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<ByteRange>,
) -> Result<ByteRange> {
    reject_duplicate(current, "--range")?;
    let value = take_value(command, args, index, "--range")?;
    parse_byte_range(&value)
}

fn finish_replay_options(state: ReplayOptionState) -> Result<ReplayOptions> {
    if state.json && state.out.is_none() {
        return Err(CorpusForgeError::invalid_argument(
            "`--json` requires `--out` for `replay` because standard output carries replayed binary bytes",
        ));
    }

    let profile = state.profile.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--profile` for `replay`")
    })?;
    let seed_source = require_seed_source(state.seed_source, "replay")?;
    let range = state.range.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--range` for `replay`")
    })?;

    Ok(ReplayOptions {
        profile,
        seed_source,
        range,
        out: state.out,
        metadata_out: state.metadata_out,
        quiet: state.quiet,
        json: state.json,
    })
}

fn parse_ci_tokenizer_options(args: &[OsString]) -> Result<CiTokenizerOptions> {
    let command_spec = find_command("ci").expect("ci command should exist");
    let mut state = CiTokenizerOptionState::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();
        index += parse_ci_tokenizer_option(command_spec, args, index, flag.as_ref(), &mut state)?;
    }

    finish_ci_tokenizer_options(state)
}

#[derive(Default)]
struct CiTokenizerOptionState {
    unicode_mode: Option<UnicodeMode>,
    output_kind: Option<UnicodeOutputKind>,
    case_count: Option<usize>,
    seed_source: Option<SeedSource>,
    command: Option<PathBuf>,
    harness_args: Vec<String>,
    report_out: Option<PathBuf>,
}

fn parse_ci_tokenizer_option(
    command_spec: &CommandSpec,
    args: &[OsString],
    index: usize,
    flag: &str,
    state: &mut CiTokenizerOptionState,
) -> Result<usize> {
    match flag {
        "--unicode" => {
            state.unicode_mode = Some(take_unicode_mode(
                command_spec,
                args,
                index,
                &state.unicode_mode,
            )?)
        }
        "--output-kind" => {
            state.output_kind = Some(take_output_kind(
                command_spec,
                args,
                index,
                &state.output_kind,
            )?)
        }
        "--cases" => {
            state.case_count = Some(take_case_count(
                command_spec,
                args,
                index,
                &state.case_count,
            )?)
        }
        "--seed" => {
            state.seed_source = Some(take_inline_seed(
                command_spec,
                args,
                index,
                flag,
                &state.seed_source,
            )?)
        }
        "--seed-file" => {
            state.seed_source = Some(take_file_seed("ci", args, index, flag, &state.seed_source)?)
        }
        "--command" => {
            state.command = Some(take_unique_path(&state.command, "ci", args, index, flag)?)
        }
        "--arg" => state
            .harness_args
            .push(take_raw_string_value("ci", args, index, "--arg")?),
        "--report-out" => {
            state.report_out = Some(take_unique_path(
                &state.report_out,
                "ci",
                args,
                index,
                flag,
            )?)
        }
        "-h" | "--help" => {
            return Err(CorpusForgeError::invalid_argument(
                "help must be requested without other arguments; run `corpusforge ci --help`",
            ))
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
    Ok(2)
}

fn finish_ci_tokenizer_options(state: CiTokenizerOptionState) -> Result<CiTokenizerOptions> {
    let mode = state.unicode_mode.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--unicode` for `ci tokenizer`")
    })?;
    let output_kind = state.output_kind.ok_or_else(|| {
        CorpusForgeError::invalid_argument(
            "missing required option `--output-kind` for `ci tokenizer`",
        )
    })?;
    let case_count = state.case_count.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--cases` for `ci tokenizer`")
    })?;
    let seed_source = require_seed_source(state.seed_source, "ci tokenizer")?;
    let command = state.command.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--command` for `ci tokenizer`")
    })?;
    let report_out = state.report_out.ok_or_else(|| {
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
        args: state.harness_args,
        report_out,
    })
}

fn parse_shrink_options(args: &[OsString]) -> Result<ShrinkOptions> {
    let command_spec = find_command("shrink").expect("shrink command should exist");
    let mut state = ShrinkOptionState::new();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].to_string_lossy();
        index += parse_shrink_option(command_spec, args, index, flag.as_ref(), &mut state)?;
    }

    finish_shrink_options(state)
}

struct ShrinkOptionState {
    input: Option<PathBuf>,
    predicate: Option<PathBuf>,
    predicate_args: Vec<String>,
    out: Option<PathBuf>,
    metadata_out: Option<PathBuf>,
    timeout_ms: Option<u64>,
    max_runs: Option<usize>,
    quiet: bool,
    json: bool,
}

impl ShrinkOptionState {
    const fn new() -> Self {
        Self {
            input: None,
            predicate: None,
            predicate_args: Vec::new(),
            out: None,
            metadata_out: None,
            timeout_ms: None,
            max_runs: None,
            quiet: false,
            json: false,
        }
    }
}

fn parse_shrink_option(
    command_spec: &CommandSpec,
    args: &[OsString],
    index: usize,
    flag: &str,
    state: &mut ShrinkOptionState,
) -> Result<usize> {
    match flag {
        "--input" => {
            state.input = Some(take_unique_path(&state.input, "shrink", args, index, flag)?)
        }
        "--predicate" => {
            state.predicate = Some(take_unique_path(
                &state.predicate,
                "shrink",
                args,
                index,
                flag,
            )?)
        }
        "--predicate-arg" => state.predicate_args.push(take_raw_string_value(
            "shrink",
            args,
            index,
            "--predicate-arg",
        )?),
        "--out" => state.out = Some(take_unique_path(&state.out, "shrink", args, index, flag)?),
        "--metadata-out" => {
            state.metadata_out = Some(take_unique_path(
                &state.metadata_out,
                "shrink",
                args,
                index,
                flag,
            )?)
        }
        "--timeout-ms" => {
            state.timeout_ms = Some(take_timeout_ms(
                command_spec,
                args,
                index,
                &state.timeout_ms,
            )?)
        }
        "--max-runs" => {
            state.max_runs = Some(take_max_runs(command_spec, args, index, &state.max_runs)?)
        }
        _ => return parse_quiet_json_or_reject("shrink", flag, &mut state.quiet, &mut state.json),
    }
    Ok(2)
}

fn parse_quiet_json_or_reject(
    command: &str,
    flag: &str,
    quiet: &mut bool,
    json: &mut bool,
) -> Result<usize> {
    match flag {
        "--quiet" => {
            claim_switch(quiet, flag)?;
            Ok(1)
        }
        "--json" => {
            claim_switch(json, flag)?;
            Ok(1)
        }
        "-h" | "--help" => Err(CorpusForgeError::invalid_argument(format!(
            "help must be requested without other arguments; run `corpusforge {command} --help`"
        ))),
        other if other.starts_with('-') => Err(CorpusForgeError::invalid_argument(format!(
            "unknown option `{other}` for `{command}`; run `corpusforge {command} --help`"
        ))),
        other => Err(CorpusForgeError::invalid_argument(format!(
            "unexpected argument `{other}` for `{command}`; run `corpusforge {command} --help`"
        ))),
    }
}

fn finish_shrink_options(state: ShrinkOptionState) -> Result<ShrinkOptions> {
    let input = state.input.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--input` for `shrink`")
    })?;
    let predicate = state.predicate.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--predicate` for `shrink`")
    })?;
    let out = state.out.ok_or_else(|| {
        CorpusForgeError::invalid_argument("missing required option `--out` for `shrink`")
    })?;

    Ok(ShrinkOptions {
        input,
        predicate,
        predicate_args: state.predicate_args,
        out,
        metadata_out: state.metadata_out,
        timeout_ms: state.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
        max_runs: state.max_runs.unwrap_or(DEFAULT_MAX_RUNS),
        quiet: state.quiet,
        json: state.json,
    })
}

fn reject_profile_only_gen_options(
    json: bool,
    metadata_out: Option<&PathBuf>,
    determinism_set: bool,
) -> Result<()> {
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

    Ok(())
}

fn take_unique_path(
    current: &Option<PathBuf>,
    command: &str,
    args: &[OsString],
    index: usize,
    flag: &str,
) -> Result<PathBuf> {
    reject_duplicate(current, flag)?;
    take_path_value(command, args, index, flag)
}

fn take_inline_seed(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    flag: &str,
    current: &Option<SeedSource>,
) -> Result<SeedSource> {
    reject_seed_conflict(current, flag)?;
    let seed = take_value(command, args, index, flag)?;
    validate_non_empty(&seed, flag)?;
    Ok(SeedSource::Inline(seed))
}

fn take_file_seed(
    command: &str,
    args: &[OsString],
    index: usize,
    flag: &str,
    current: &Option<SeedSource>,
) -> Result<SeedSource> {
    reject_seed_conflict(current, flag)?;
    Ok(SeedSource::File(take_path_value(
        command, args, index, flag,
    )?))
}

fn take_byte_count(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<usize>,
) -> Result<usize> {
    reject_duplicate(current, "--bytes")?;
    let value = take_value(command, args, index, "--bytes")?;
    let bytes = parse_byte_size(&value)?;
    usize::try_from(bytes).map_err(|_| {
        CorpusForgeError::invalid_argument(format!(
            "byte size `{value}` exceeds this platform's maximum supported output size"
        ))
    })
}

fn take_unicode_mode(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<UnicodeMode>,
) -> Result<UnicodeMode> {
    reject_duplicate(current, "--unicode")?;
    let value = take_value(command, args, index, "--unicode")?;
    UnicodeMode::from_str(&value)
}

fn take_output_kind(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<UnicodeOutputKind>,
) -> Result<UnicodeOutputKind> {
    reject_duplicate(current, "--output-kind")?;
    let value = take_value(command, args, index, "--output-kind")?;
    UnicodeOutputKind::from_str(&value)
}

fn take_grammar_format(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<GrammarFormat>,
) -> Result<GrammarFormat> {
    reject_duplicate(current, "--grammar")?;
    let value = take_value(command, args, index, "--grammar")?;
    GrammarFormat::from_str(&value)
}

fn take_grammar_mode(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<GrammarMode>,
) -> Result<GrammarMode> {
    reject_duplicate(current, "--grammar-mode")?;
    let value = take_value(command, args, index, "--grammar-mode")?;
    GrammarMode::from_str(&value)
}

fn take_case_count(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<usize>,
) -> Result<usize> {
    reject_duplicate(current, "--cases")?;
    let value = take_value(command, args, index, "--cases")?;
    parse_case_count(&value)
}

fn take_timeout_ms(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<u64>,
) -> Result<u64> {
    reject_duplicate(current, "--timeout-ms")?;
    let value = take_raw_string_value(command.name, args, index, "--timeout-ms")?;
    parse_timeout_ms(&value)
}

fn take_max_runs(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    current: &Option<usize>,
) -> Result<usize> {
    reject_duplicate(current, "--max-runs")?;
    let value = take_raw_string_value(command.name, args, index, "--max-runs")?;
    parse_max_runs(&value)
}

fn set_gen_determinism(
    command: &CommandSpec,
    args: &[OsString],
    index: usize,
    state: &mut GenOptionState,
) -> Result<()> {
    if state.determinism_set {
        return Err(CorpusForgeError::invalid_argument(
            "duplicate option `--determinism`",
        ));
    }

    let value = take_value(command, args, index, "--determinism")?;
    state.determinism = parse_determinism_mode(&value)?;
    state.determinism_set = true;
    Ok(())
}

fn reject_duplicate<T>(current: &Option<T>, flag: &str) -> Result<()> {
    if current.is_some() {
        return Err(CorpusForgeError::invalid_argument(format!(
            "duplicate option `{flag}`"
        )));
    }
    Ok(())
}

fn claim_switch(current: &mut bool, flag: &str) -> Result<()> {
    if *current {
        return Err(CorpusForgeError::invalid_argument(format!(
            "duplicate option `{flag}`"
        )));
    }
    *current = true;
    Ok(())
}

fn reject_seed_conflict(current: &Option<SeedSource>, flag: &str) -> Result<()> {
    if let Some(existing) = seed_source_name(current) {
        return Err(seed_conflict(flag, existing));
    }
    Ok(())
}

fn require_seed_source(seed_source: Option<SeedSource>, command: &str) -> Result<SeedSource> {
    seed_source.ok_or_else(|| {
        CorpusForgeError::invalid_argument(format!(
            "missing required seed source for `{command}`; use exactly one of `--seed` or `--seed-file`",
        ))
    })
}

fn seed_source_name(seed_source: &Option<SeedSource>) -> Option<&'static str> {
    match seed_source {
        Some(SeedSource::Inline(_)) => Some("--seed"),
        Some(SeedSource::File(_)) => Some("--seed-file"),
        None => None,
    }
}

fn seed_conflict(flag: &str, existing: &str) -> CorpusForgeError {
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

fn execute_command(command: ParsedCommand, stdout: &mut impl Write) -> Result<Option<String>> {
    match command {
        ParsedCommand::TopHelp => Ok(Some(top_level_help())),
        ParsedCommand::Version => Ok(Some(version_text())),
        ParsedCommand::CommandHelp(command) => Ok(Some(command_help(command))),
        ParsedCommand::ExecutePlaceholder(command) => Err(CorpusForgeError::not_implemented(
            format!("{} command execution", command.name),
        )),
        ParsedCommand::ExecuteCiTokenizer(options) => execute_ci_tokenizer(options).map(Some),
        ParsedCommand::ExecuteGen(options) => execute_gen(options, stdout),
        ParsedCommand::ExecuteReplay(options) => execute_replay(options, stdout),
        ParsedCommand::ExecuteShrink(options) => execute_shrink(options).map(Some),
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

fn execute_gen(options: GenOptions, stdout: &mut impl Write) -> Result<Option<String>> {
    match options {
        GenOptions::Profile(options) => execute_profile_gen(options, stdout),
        GenOptions::Unicode(options) => execute_unicode_gen(options, stdout),
        GenOptions::Grammar(options) => execute_grammar_gen(options, stdout),
    }
}

fn execute_profile_gen(
    options: ProfileGenOptions,
    stdout: &mut impl Write,
) -> Result<Option<String>> {
    let seed = read_seed(&options.seed_source)?;
    let LoadedProfileModel {
        profile_hash,
        model,
    } = load_profile_model(&options.profile)?;

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

fn execute_replay(options: ReplayOptions, stdout: &mut impl Write) -> Result<Option<String>> {
    let seed = read_seed(&options.seed_source)?;
    let LoadedProfileModel {
        profile_hash,
        model,
    } = load_profile_model(&options.profile)?;
    let replay_end = usize::try_from(options.range.end).map_err(|_| {
        CorpusForgeError::invalid_argument(
            "`--range` end exceeds this platform's supported replay byte count",
        )
    })?;

    if let Some(out) = &options.out {
        let file = File::create(out)?;
        let mut range_writer = ByteRangeWriter::new(file, options.range.start, options.range.end);
        model.generate_bytes(&seed, replay_end, &mut range_writer)?;
        range_writer.flush()?;
    } else {
        let mut range_writer = ByteRangeWriter::new(stdout, options.range.start, options.range.end);
        model.generate_bytes(&seed, replay_end, &mut range_writer)?;
        range_writer.flush()?;
    }

    if let Some(metadata_out) = &options.metadata_out {
        fs::write(
            metadata_out,
            format_replay_metadata(&options, &seed, &profile_hash),
        )?;
    }

    if options.out.is_none() || options.quiet {
        return Ok(None);
    }

    Ok(Some(format_replay_summary(&options, &seed, &profile_hash)))
}

struct LoadedProfileModel {
    profile_hash: String,
    model: ByteBigramModel,
}

fn load_profile_model(profile: &Path) -> Result<LoadedProfileModel> {
    let profile_bytes = fs::read(profile)?;
    let pack = ProfilePack::from_bytes(&profile_bytes)?;
    let profile_hash = pack.profile_hash();
    let model_bytes = pack.ngram_model_bytes().ok_or_else(|| {
        CorpusForgeError::invalid_profile(
            "profile lacks required NGRAMV0\\0 n-gram model section; rebuild it with `corpusforge profile build <input> --out <path>`",
        )
    })?;
    let model = ByteBigramModel::from_bytes(model_bytes)?;

    Ok(LoadedProfileModel {
        profile_hash,
        model,
    })
}

fn execute_unicode_gen(
    options: UnicodeGenOptions,
    stdout: &mut impl Write,
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

fn execute_grammar_gen(
    options: GrammarGenOptions,
    stdout: &mut impl Write,
) -> Result<Option<String>> {
    let seed = read_seed(&options.seed_source)?;
    let spec = GrammarCaseSpec::new(
        options.format,
        options.mode,
        options.case_count,
        options.unicode_mode,
    )?;
    let cases = generate_grammar_cases(&seed, spec)?;
    let bytes = join_grammar_cases(&cases);

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

    Ok(Some(format_grammar_gen_summary(
        &options,
        &seed,
        bytes.len(),
    )))
}

fn join_grammar_cases(cases: &[GrammarCase]) -> Vec<u8> {
    let mut bytes = Vec::new();

    for (index, case) in cases.iter().enumerate() {
        if index > 0 {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(case.text().as_bytes());
    }

    bytes
}

fn execute_shrink(options: ShrinkOptions) -> Result<String> {
    let input = fs::read(&options.input)?;
    let predicate =
        PredicateCommand::new(options.predicate.clone(), options.predicate_args.clone());
    let config = ShrinkConfig::new(predicate)
        .with_timeout_ms(options.timeout_ms)
        .with_max_runs(options.max_runs);
    let outcome = shrink_bytes(&input, &config)?;

    let mut file = File::create(&options.out)?;
    file.write_all(outcome.reduced_bytes())?;
    file.flush()?;

    if let Some(metadata_out) = &options.metadata_out {
        fs::write(metadata_out, format_shrink_metadata(&options, &outcome))?;
    }

    if options.quiet {
        return Ok(String::new());
    }

    Ok(format_shrink_summary(&options, &outcome))
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

fn format_replay_summary(options: &ReplayOptions, seed: &MasterSeed, profile_hash: &str) -> String {
    let out = options
        .out
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "stdout".to_owned());

    if options.json {
        return format!(
            "{{\"command\":\"replay\",\"seed\":\"{}\",\"profile_hash\":\"{}\",\"engine_name\":\"{}\",\"engine_version\":{},\"range_start\":{},\"range_end\":{},\"byte_count\":{},\"out\":\"{}\"}}",
            seed,
            json_escape(profile_hash),
            json_escape(ENGINE_NAME),
            ENGINE_VERSION,
            options.range.start,
            options.range.end,
            options.range.byte_count(),
            json_escape(&out)
        );
    }

    format!(
        "replayed corpus range\nprofile_hash: {profile_hash}\nseed: {seed}\nengine: {engine_name}/{engine_version}\nrange_start: {range_start}\nrange_end: {range_end}\nbyte_count: {byte_count}\nout: {out}",
        engine_name = ENGINE_NAME,
        engine_version = ENGINE_VERSION,
        range_start = options.range.start,
        range_end = options.range.end,
        byte_count = options.range.byte_count()
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

fn format_grammar_gen_summary(
    options: &GrammarGenOptions,
    seed: &MasterSeed,
    byte_count: usize,
) -> String {
    let out = options
        .out
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "stdout".to_owned());
    let unicode_mode = options
        .unicode_mode
        .map(|mode| mode.to_string())
        .unwrap_or_else(|| "none".to_owned());

    format!(
        "generated grammar corpus\nseed: {seed}\ngrammar: {format}\ngrammar_mode: {mode}\nunicode_mode: {unicode_mode}\ncase_count: {case_count}\nbyte_count: {byte_count}\nout: {out}",
        format = options.format,
        mode = options.mode,
        case_count = options.case_count
    )
}

fn format_shrink_summary(options: &ShrinkOptions, outcome: &ShrinkOutcome) -> String {
    if options.json {
        return format!(
            "{{\"tool_version\":\"{}\",\"command\":\"shrink\",\"input_byte_count\":{},\"reduced_byte_count\":{},\"predicate_runs\":{},\"failure_kind\":\"{}\",\"original_hash\":\"{}\",\"reduced_hash\":\"{}\",\"timeout_ms\":{},\"max_runs\":{},\"output_mode\":\"file\",\"out\":\"{}\"}}",
            json_escape(env!("CARGO_PKG_VERSION")),
            outcome.original_byte_count(),
            outcome.reduced_byte_count(),
            outcome.predicate_runs(),
            format_failure_kind(outcome.failure_kind()),
            json_escape(outcome.original_hash()),
            json_escape(outcome.reduced_hash()),
            options.timeout_ms,
            options.max_runs,
            json_escape(&options.out.display().to_string())
        );
    }

    format!(
        "shrunk failing input\ninput_byte_count: {input_byte_count}\nreduced_byte_count: {reduced_byte_count}\npredicate_runs: {predicate_runs}\nfailure_kind: {failure_kind}\noriginal_hash: {original_hash}\nreduced_hash: {reduced_hash}\ntimeout_ms: {timeout_ms}\nmax_runs: {max_runs}\nout: {out}",
        input_byte_count = outcome.original_byte_count(),
        reduced_byte_count = outcome.reduced_byte_count(),
        predicate_runs = outcome.predicate_runs(),
        failure_kind = format_failure_kind(outcome.failure_kind()),
        original_hash = outcome.original_hash(),
        reduced_hash = outcome.reduced_hash(),
        timeout_ms = options.timeout_ms,
        max_runs = options.max_runs,
        out = options.out.display()
    )
}

fn format_shrink_metadata(options: &ShrinkOptions, outcome: &ShrinkOutcome) -> String {
    format!(
        "{{\"tool_version\":\"{}\",\"command\":\"shrink\",\"input_byte_count\":{},\"reduced_byte_count\":{},\"predicate_runs\":{},\"failure_kind\":\"{}\",\"original_hash\":\"{}\",\"reduced_hash\":\"{}\",\"timeout_ms\":{},\"max_runs\":{},\"output_mode\":\"file\",\"out\":\"{}\"}}\n",
        json_escape(env!("CARGO_PKG_VERSION")),
        outcome.original_byte_count(),
        outcome.reduced_byte_count(),
        outcome.predicate_runs(),
        format_failure_kind(outcome.failure_kind()),
        json_escape(outcome.original_hash()),
        json_escape(outcome.reduced_hash()),
        options.timeout_ms,
        options.max_runs,
        json_escape(&options.out.display().to_string())
    )
}

fn format_failure_kind(kind: PredicateFailureKind) -> String {
    match kind {
        PredicateFailureKind::ExitCode(code) => format!("exit_code:{code}"),
        PredicateFailureKind::Timeout => "timeout".to_owned(),
    }
}

fn format_gen_metadata(
    options: &ProfileGenOptions,
    seed: &MasterSeed,
    profile_hash: &str,
) -> String {
    format!(
        "{{\"tool_version\":\"{}\",\"command\":\"gen\",\"seed\":\"{}\",\"profile_hash\":\"{}\",\"engine_name\":\"{}\",\"engine_version\":{},\"byte_count\":{},\"determinism\":\"{}\",\"output_mode\":\"{}\",\"json_summary\":{},\"quiet\":{}}}\n",
        json_escape(env!("CARGO_PKG_VERSION")),
        seed,
        json_escape(profile_hash),
        json_escape(ENGINE_NAME),
        ENGINE_VERSION,
        options.byte_count,
        options.determinism.as_str(),
        output_mode(&options.out),
        options.json,
        options.quiet
    )
}

fn format_replay_metadata(
    options: &ReplayOptions,
    seed: &MasterSeed,
    profile_hash: &str,
) -> String {
    format!(
        "{{\"tool_version\":\"{}\",\"command\":\"replay\",\"seed\":\"{}\",\"profile_hash\":\"{}\",\"engine_name\":\"{}\",\"engine_version\":{},\"range_start\":{},\"range_end\":{},\"byte_count\":{},\"output_mode\":\"{}\",\"quiet\":{},\"json\":{}}}\n",
        json_escape(env!("CARGO_PKG_VERSION")),
        seed,
        json_escape(profile_hash),
        json_escape(ENGINE_NAME),
        ENGINE_VERSION,
        options.range.start,
        options.range.end,
        options.range.byte_count(),
        output_mode(&options.out),
        options.quiet,
        options.json
    )
}

fn output_mode(out: &Option<PathBuf>) -> &'static str {
    if out.is_some() {
        "file"
    } else {
        "stdout"
    }
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

fn parse_byte_range(value: &str) -> Result<ByteRange> {
    let Some((start, end)) = value.split_once("..") else {
        return Err(invalid_range(value));
    };

    if start.contains("..") || end.contains("..") {
        return Err(invalid_range(value));
    }

    let start = parse_range_endpoint(value, start, "start")?;
    let end = parse_range_endpoint(value, end, "end")?;

    if end < start {
        return Err(CorpusForgeError::invalid_argument(format!(
            "invalid range `{value}`; range end must be greater than or equal to range start"
        )));
    }

    Ok(ByteRange { start, end })
}

fn parse_range_endpoint(range: &str, endpoint: &str, label: &str) -> Result<u64> {
    if endpoint.is_empty() || !endpoint.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_range(range));
    }

    endpoint.parse::<u64>().map_err(|_| {
        CorpusForgeError::invalid_argument(format!(
            "invalid range `{range}`; {label} endpoint is too large"
        ))
    })
}

fn invalid_range(value: &str) -> CorpusForgeError {
    CorpusForgeError::invalid_argument(format!(
        "invalid range `{value}`; expected decimal unsigned integers as `start..end`"
    ))
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

fn parse_timeout_ms(value: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CorpusForgeError::invalid_argument(format!(
            "invalid timeout `{value}`; expected a positive integer for `--timeout-ms`"
        )));
    }

    let parsed = value.parse::<u64>().map_err(|_| {
        CorpusForgeError::invalid_argument(format!(
            "invalid timeout `{value}`; expected a positive integer for `--timeout-ms`"
        ))
    })?;

    if parsed == 0 {
        return Err(CorpusForgeError::invalid_argument(
            "`--timeout-ms` must be greater than zero",
        ));
    }

    Ok(parsed)
}

fn parse_max_runs(value: &str) -> Result<usize> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CorpusForgeError::invalid_argument(format!(
            "invalid run limit `{value}`; expected a positive integer for `--max-runs`"
        )));
    }

    let parsed = value.parse::<usize>().map_err(|_| {
        CorpusForgeError::invalid_argument(format!(
            "invalid run limit `{value}`; expected a positive integer for `--max-runs`"
        ))
    })?;

    if parsed < 2 {
        return Err(CorpusForgeError::invalid_argument(
            "`--max-runs` must be at least 2 to confirm the original failure",
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

    if command.name == "shrink" {
        return shrink_help(command);
    }

    if command.name == "replay" {
        return replay_help(command);
    }

    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge {name} [OPTIONS]\n\nOPTIONS:\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a file\n    --profile <path>              Read a CorpusForge profile path\n    --out <path>                  Write generated output to a path\n    --bytes <N>                   Set output size in bytes; supports KB, MB, GB\n    --determinism <mode>          Determinism mode: strict or relaxed\n    --metadata-out <path>         Write machine-readable metadata to a path\n    --quiet                       Reduce human-readable output\n    --json                        Emit machine-readable JSON where supported\n    -h, --help                    Print help\n\nEXAMPLES:\n    corpusforge {name} --seed 42 --profile profiles/smoke.cff --bytes 64KB\n    corpusforge {name} --seed-file seed.txt --determinism strict --metadata-out report.json --json\n\nSTATUS:\n    Planned for a later milestone; execution currently returns NotImplemented.",
        name = command.name,
        summary = command.summary
    )
}

fn shrink_help(command: &CommandSpec) -> String {
    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge shrink --input <path> --predicate <path> [--predicate-arg <value> ...] --out <path> [OPTIONS]\n\nOPTIONS:\n    --input <path>                Read the original failing input bytes\n    --predicate <path>            Predicate executable path; invoked directly without a shell\n    --predicate-arg <value>       Literal predicate argument; may be repeated and preserves order\n    --out <path>                  Write reduced failing bytes to a path\n    --metadata-out <path>         Write stable shrink metadata JSON to a path\n    --timeout-ms <N>              Per-run predicate timeout in milliseconds; default 1000\n    --max-runs <N>                Maximum predicate executions; default 10000, minimum 2\n    --quiet                       Suppress stdout summary after writing outputs\n    --json                        Emit stable JSON summary on stdout\n    -h, --help                    Print help\n\nPREDICATE:\n    The predicate reads candidate bytes from stdin. Exit code 0 means the candidate passed; a non-zero exit or timeout is treated as a failure signature to preserve.",
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
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge gen --profile <path> (--seed <seed> | --seed-file <path>) --bytes <N> [OPTIONS]\n    corpusforge gen --unicode <mode> --output-kind <valid-text|raw-bytes> --cases <N> (--seed <seed> | --seed-file <path>) [--out <path>] [--quiet]\n    corpusforge gen --grammar <markdown|json> --grammar-mode <valid|near-valid|malformed> --cases <N> (--seed <seed> | --seed-file <path>) [--unicode <mode>] [--out <path>] [--quiet]\n\nOPTIONS:\n    --profile <path>              Read a CorpusForge .cff profile with an embedded n-gram model\n    --grammar <format>            Generate grammar-backed valid UTF-8 text cases: markdown or json\n    --grammar-mode <mode>         Grammar validity mode: valid, near-valid, or malformed\n    --unicode <mode>              Generate Unicode cases, or compose valid-text Unicode into grammar leaves\n    --output-kind <kind>          Unicode-only output boundary: valid-text or raw-bytes\n    --cases <N>                   Number of Unicode or grammar cases to generate\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a 32-byte file\n    --bytes <N>                   Set profile-backed output size in bytes; supports KB, MB, GB\n    --out <path>                  Stream generated bytes to a file instead of stdout\n    --determinism <mode>          Profile-backed determinism mode: strict or relaxed\n    --metadata-out <path>         Write profile-backed stable generation metadata JSON to a path\n    --quiet                       Suppress human-readable output when --out is used\n    --json                        Emit profile-backed JSON summary when --out is used\n    -h, --help                    Print help\n\nUNICODE MODES:\n    grapheme, bidi, zero-width, emoji, normalization, mixed, invalid-utf8\n\nGRAMMAR MODES:\n    markdown, json with valid, near-valid, or malformed text output. Grammar output is valid UTF-8; invalid-utf8 Unicode composition is rejected.\n\nSTDOUT:\n    Without --out, generated binary bytes are written directly to stdout without an added trailing newline for profile and Unicode-only paths. Grammar output is valid UTF-8 text written as bytes. Use --out before --json for profile-backed generation.",
        name = command.name,
        summary = command.summary
    )
}

fn replay_help(command: &CommandSpec) -> String {
    format!(
        "corpusforge {name}\n\n{summary}\n\nUSAGE:\n    corpusforge replay --profile <path> (--seed <seed> | --seed-file <path>) --range <start>..<end> [--out <path>] [--metadata-out <path>] [--quiet] [--json]\n\nOPTIONS:\n    --profile <path>              Read a CorpusForge .cff profile with an embedded n-gram model\n    --seed <seed>                 Use an inline deterministic seed\n    --seed-file <path>            Read the deterministic seed from a 32-byte file\n    --range <start>..<end>        Replay the half-open byte range; empty ranges are allowed\n    --out <path>                  Stream replayed bytes to a file instead of stdout\n    --metadata-out <path>         Write stable replay metadata JSON to a path\n    --quiet                       Suppress stdout summary when --out is used\n    --json                        Emit replay JSON summary when --out is used\n    -h, --help                    Print help\n\nSTDOUT:\n    Without --out, replayed binary bytes are written directly to stdout without an added trailing newline. Use --out before --json.",
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
    use corpusforge_testkit::bytes_to_hex;
    use std::io::{self, Read};
    use std::path::Path;

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
            if command != "shrink" {
                assert!(help.contains("--profile <path>"));
            }
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
            } else if command == "shrink" {
                assert!(help.contains("--input <path>"));
                assert!(help.contains("--predicate <path>"));
                assert!(help.contains("--predicate-arg <value>"));
                assert!(help.contains("--out <path>"));
                assert!(help.contains("--metadata-out <path>"));
                assert!(help.contains("--timeout-ms <N>"));
                assert!(help.contains("--max-runs <N>"));
                assert!(help.contains("--quiet"));
                assert!(help.contains("--json"));
                assert!(help.contains("reads candidate bytes from stdin"));
                assert!(!help.contains("Planned for a later milestone"));
            } else if command == "replay" {
                assert!(help.contains("--seed <seed>"));
                assert!(help.contains("--seed-file <path>"));
                assert!(help.contains("--range <start>..<end>"));
                assert!(help.contains("--out <path>"));
                assert!(help.contains("--metadata-out <path>"));
                assert!(help.contains("--quiet"));
                assert!(help.contains("--json"));
                assert!(help.contains("replayed binary bytes"));
                assert!(!help.contains("--bytes <N>"));
                assert!(!help.contains("--determinism <mode>"));
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
                    assert!(help.contains("--grammar <format>"));
                    assert!(help.contains("--grammar-mode <mode>"));
                    assert!(help.contains("Grammar output is valid UTF-8"));
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
        assert!(help.contains("--grammar <format>"));
        assert!(help.contains("--grammar-mode <mode>"));
        assert!(help.contains("--unicode <mode>"));
        assert!(help.contains("generated binary bytes"));
        assert!(!help.contains("Planned for a later milestone"));
    }

    #[test]
    fn replay_requires_profile_seed_and_range() {
        let cases = [
            (
                &["corpusforge", "replay", "--seed", "42", "--range", "0..8"][..],
                "missing required option `--profile`",
            ),
            (
                &[
                    "corpusforge",
                    "replay",
                    "--profile",
                    "profiles/smoke.cff",
                    "--range",
                    "0..8",
                ][..],
                "missing required seed source",
            ),
            (
                &[
                    "corpusforge",
                    "replay",
                    "--profile",
                    "profiles/smoke.cff",
                    "--seed",
                    "42",
                ][..],
                "missing required option `--range`",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn replay_rejects_duplicate_and_conflicting_flags() {
        let cases = [
            (
                &[
                    "corpusforge",
                    "replay",
                    "--profile",
                    "a.cff",
                    "--profile",
                    "b.cff",
                ][..],
                "duplicate option `--profile`",
            ),
            (
                &[
                    "corpusforge",
                    "replay",
                    "--range",
                    "0..1",
                    "--range",
                    "1..2",
                ][..],
                "duplicate option `--range`",
            ),
            (
                &[
                    "corpusforge",
                    "replay",
                    "--seed",
                    "1",
                    "--seed-file",
                    "seed.txt",
                ][..],
                "conflicts",
            ),
            (
                &["corpusforge", "replay", "--quiet", "--quiet"][..],
                "duplicate option `--quiet`",
            ),
            (
                &["corpusforge", "replay", "--json", "--json"][..],
                "duplicate option `--json`",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn replay_rejects_invalid_ranges() {
        let cases = [
            (
                &["corpusforge", "replay", "--range", "0"][..],
                "invalid range",
            ),
            (
                &["corpusforge", "replay", "--range", "..1"][..],
                "invalid range",
            ),
            (
                &["corpusforge", "replay", "--range", "1.."][..],
                "invalid range",
            ),
            (
                &["corpusforge", "replay", "--range", "a..1"][..],
                "invalid range",
            ),
            (
                &["corpusforge", "replay", "--range", "2..1"][..],
                "range end must be greater than or equal",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn replay_rejects_json_without_out() {
        let CliOutcome::Failure(error) = run(["corpusforge", "replay"]) else {
            panic!("missing options should fail");
        };

        assert_eq!(error.category(), "invalid_argument");
        assert!(error
            .to_string()
            .contains("missing required option `--profile`"));

        let CliOutcome::Failure(error) = run([
            "corpusforge",
            "replay",
            "--profile",
            "profiles/smoke.cff",
            "--seed",
            "42",
            "--range",
            "0..8",
            "--json",
        ]) else {
            panic!("json without out should fail");
        };

        assert_eq!(error.category(), "invalid_argument");
        assert!(error
            .to_string()
            .contains("standard output carries replayed binary bytes"));
    }

    #[test]
    fn replay_rejects_placeholder_only_flags() {
        let cases = [
            (
                &["corpusforge", "replay", "--bytes", "64KB"][..],
                "unknown option `--bytes`",
            ),
            (
                &["corpusforge", "replay", "--determinism", "strict"][..],
                "unknown option `--determinism`",
            ),
            (
                &["corpusforge", "replay", "--unknown"][..],
                "unknown option",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn shrink_requires_input_predicate_and_out() {
        let cases = [
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                ][..],
                "--input",
            ),
            (
                &["corpusforge", "shrink", "--input", "input", "--out", "out"][..],
                "--predicate",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                ][..],
                "--out",
            ),
        ];

        for (args, expected) in cases {
            let CliOutcome::Failure(error) = run(args) else {
                panic!("{args:?} should fail");
            };

            assert_eq!(error.category(), "invalid_argument");
            assert!(error.to_string().contains(expected));
        }
    }

    #[test]
    fn shrink_rejects_duplicates_and_invalid_limits() {
        let cases = [
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "a",
                    "--input",
                    "b",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                ][..],
                "duplicate option `--input`",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--timeout-ms",
                    "0",
                ][..],
                "`--timeout-ms` must be greater than zero",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--timeout-ms",
                    "1.5",
                ][..],
                "invalid timeout",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--timeout-ms",
                    "-1",
                ][..],
                "invalid timeout",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--max-runs",
                    "1",
                ][..],
                "`--max-runs` must be at least 2",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--max-runs",
                    "many",
                ][..],
                "invalid run limit",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--max-runs",
                    "-2",
                ][..],
                "invalid run limit",
            ),
            (
                &[
                    "corpusforge",
                    "shrink",
                    "--input",
                    "input",
                    "--predicate",
                    "pred",
                    "--out",
                    "out",
                    "--json",
                    "--json",
                ][..],
                "duplicate option `--json`",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn shrink_json_summary_and_metadata_are_stable() {
        let input = temp_report_path("shrink-input.txt");
        let out = temp_report_path("shrink-out.txt");
        let metadata = temp_report_path("shrink-metadata.json");
        std::fs::write(&input, b"prefix fail suffix").expect("input should be written");

        let out_text = out.display().to_string();
        let metadata_text = metadata.display().to_string();

        let outcome = run(extend_owned_args(
            shrink_test_args(&input, &out),
            [
                "--metadata-out",
                &metadata_text,
                "--timeout-ms",
                "1000",
                "--max-runs",
                "100",
                "--json",
            ],
        ));

        let CliOutcome::Success(summary) = outcome else {
            panic!("shrink should succeed");
        };

        assert_eq!(std::fs::read(&out).expect("out should be written"), b"fail");
        assert!(summary.starts_with(&format!(
            "{{\"tool_version\":\"{}\",\"command\":\"shrink\",\"input_byte_count\":18,\"reduced_byte_count\":4,\"predicate_runs\":",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(summary.contains("\"failure_kind\":\"exit_code:101\""));
        assert!(summary.contains("\"timeout_ms\":1000"));
        assert!(summary.contains("\"max_runs\":100"));
        assert!(summary.ends_with(&format!(
            "\"output_mode\":\"file\",\"out\":\"{}\"}}",
            json_escape_for_test(&out_text)
        )));

        let metadata_json = std::fs::read_to_string(&metadata).expect("metadata should be written");
        assert_eq!(metadata_json, format!("{summary}\n"));

        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(metadata);
    }

    #[test]
    fn shrink_quiet_suppresses_summary_but_writes_output() {
        let input = temp_report_path("shrink-quiet-input.txt");
        let out = temp_report_path("shrink-quiet-out.txt");
        std::fs::write(&input, b"before fail after").expect("input should be written");

        let outcome = run(extend_owned_args(
            shrink_test_args(&input, &out),
            ["--quiet", "--json"],
        ));

        let CliOutcome::Success(summary) = outcome else {
            panic!("shrink should succeed");
        };

        assert!(summary.is_empty());
        assert_eq!(std::fs::read(&out).expect("out should be written"), b"fail");

        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_file(out);
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

        assert_invalid_argument_cases(&cases);
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

        assert_invalid_argument_cases(&cases);
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

        assert_invalid_argument_cases(&cases);
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

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn gen_grammar_requires_format_mode_cases_and_seed() {
        let cases = [
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                ][..],
                "missing required option `--grammar`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                ][..],
                "missing required option `--grammar-mode`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--seed",
                    "1337",
                ][..],
                "missing required option `--cases`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
                ][..],
                "missing required seed source",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn gen_grammar_rejects_invalid_values_and_invalid_utf8_composition() {
        let cases = [
            (
                &["corpusforge", "gen", "--grammar", "xml"][..],
                "unsupported grammar format",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "almost",
                ][..],
                "unsupported grammar mode",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "json",
                    "--grammar-mode",
                    "malformed",
                    "--unicode",
                    "invalid-utf8",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                ][..],
                "cannot be composed into grammar output",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn gen_grammar_rejects_mixed_paths_and_profile_only_options() {
        let cases = [
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                    "--profile",
                    "profiles/smoke.cff",
                ][..],
                "cannot be mixed",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                    "--output-kind",
                    "valid-text",
                ][..],
                "cannot be mixed with Unicode-only `--output-kind`",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
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
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                    "--metadata-out",
                    "metadata.json",
                ][..],
                "only supported for profile-backed",
            ),
            (
                &[
                    "corpusforge",
                    "gen",
                    "--grammar",
                    "markdown",
                    "--grammar-mode",
                    "valid",
                    "--cases",
                    "8",
                    "--seed",
                    "1337",
                    "--determinism",
                    "strict",
                ][..],
                "only supported for profile-backed",
            ),
        ];

        assert_invalid_argument_cases(&cases);
    }

    #[test]
    fn gen_grammar_stdout_is_deterministic_valid_utf8_bytes() {
        let args = [
            "corpusforge",
            "gen",
            "--grammar",
            "markdown",
            "--grammar-mode",
            "valid",
            "--cases",
            "8",
            "--seed",
            "1337",
        ];
        let CliOutcome::SuccessBytes(first) = run(args) else {
            panic!("grammar generation should write bytes");
        };
        let CliOutcome::SuccessBytes(second) = run(args) else {
            panic!("grammar generation should write bytes");
        };

        assert_eq!(first, second);
        let text = std::str::from_utf8(&first).expect("grammar output should be valid UTF-8");
        assert!(text.contains("Case 0"));
        assert!(text.contains("mode: valid") || text.contains("case | 1"));
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

        assert_invalid_argument_cases(&cases);
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

    #[test]
    #[ignore]
    fn shrink_harness_fails_on_fail_substring() {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .expect("helper should read stdin");

        assert!(!input.windows(4).any(|window| window == b"fail"));
    }

    fn temp_report_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("corpusforge-cli-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temporary report directory should be writable");

        dir.join(format!("corpusforge-cli-{name}.json"))
    }

    fn shrink_test_args(input: &Path, out: &Path) -> Vec<String> {
        vec![
            "corpusforge".to_owned(),
            "shrink".to_owned(),
            "--input".to_owned(),
            input.display().to_string(),
            "--predicate".to_owned(),
            std::env::current_exe()
                .expect("test executable should exist")
                .display()
                .to_string(),
            "--predicate-arg".to_owned(),
            "--ignored".to_owned(),
            "--predicate-arg".to_owned(),
            "--exact".to_owned(),
            "--predicate-arg".to_owned(),
            "tests::shrink_harness_fails_on_fail_substring".to_owned(),
            "--out".to_owned(),
            out.display().to_string(),
        ]
    }

    fn extend_owned_args<const N: usize>(mut args: Vec<String>, extra: [&str; N]) -> Vec<String> {
        args.extend(extra.into_iter().map(str::to_owned));
        args
    }

    fn json_escape_for_test(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn assert_invalid_argument_cases(cases: &[(&[&str], &str)]) {
        for (args, expected) in cases {
            let CliOutcome::Failure(error) = run(*args) else {
                panic!("{args:?} should fail");
            };

            assert_eq!(error.category(), "invalid_argument");
            assert!(
                error.to_string().contains(expected),
                "{error} should contain {expected}"
            );
        }
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
