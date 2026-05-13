// SPDX-License-Identifier: Apache-2.0

//! Deterministic grammar-aware text fixture generation.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use corpusforge_core::rng::{DeterministicStream, DOMAIN_GRAMMAR};
use corpusforge_core::seed::MasterSeed;
use corpusforge_core::{CorpusForgeError, Result};
use corpusforge_unicode::{generate_valid_text, UnicodeMode};

/// Grammar fixture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GrammarFormat {
    /// Markdown-like text fixtures.
    Markdown,
    /// JSON-like text fixtures.
    Json,
}

impl GrammarFormat {
    /// Stable format order used by diagnostics and tests.
    pub const ALL: [Self; 2] = [Self::Markdown, Self::Json];

    /// Returns the stable label used in profile-adjacent APIs and diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
        }
    }
}

impl Display for GrammarFormat {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl FromStr for GrammarFormat {
    type Err = CorpusForgeError;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            _ => Err(CorpusForgeError::invalid_argument(format!(
                "unsupported grammar format `{text}`; expected one of: {}",
                stable_labels(&Self::ALL)
            ))),
        }
    }
}

/// Grammar fixture validity mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GrammarMode {
    /// Syntactically valid-looking fixtures.
    Valid,
    /// Mostly valid fixtures with suspicious but text-safe edge cases.
    NearValid,
    /// Intentionally broken fixtures that remain valid UTF-8 text.
    Malformed,
}

impl GrammarMode {
    /// Stable mode order used by diagnostics and tests.
    pub const ALL: [Self; 3] = [Self::Valid, Self::NearValid, Self::Malformed];

    /// Returns the stable label used in profile-adjacent APIs and diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::NearValid => "near-valid",
            Self::Malformed => "malformed",
        }
    }
}

impl Display for GrammarMode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl FromStr for GrammarMode {
    type Err = CorpusForgeError;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "valid" => Ok(Self::Valid),
            "near-valid" => Ok(Self::NearValid),
            "malformed" => Ok(Self::Malformed),
            _ => Err(CorpusForgeError::invalid_argument(format!(
                "unsupported grammar mode `{text}`; expected one of: {}",
                stable_labels(&Self::ALL)
            ))),
        }
    }
}

/// Validated grammar case generation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GrammarCaseSpec {
    format: GrammarFormat,
    mode: GrammarMode,
    case_count: usize,
    unicode_mode: Option<UnicodeMode>,
}

impl GrammarCaseSpec {
    /// Builds a validated grammar case specification.
    pub fn new(
        format: GrammarFormat,
        mode: GrammarMode,
        case_count: usize,
        unicode_mode: Option<UnicodeMode>,
    ) -> Result<Self> {
        if case_count == 0 {
            return Err(CorpusForgeError::invalid_argument(
                "grammar case generation requires a non-zero case count",
            ));
        }

        if unicode_mode == Some(UnicodeMode::InvalidUtf8) {
            return Err(CorpusForgeError::invalid_argument(
                "Unicode mode `invalid-utf8` cannot be composed into grammar output because grammar cases are valid UTF-8 text",
            ));
        }

        Ok(Self {
            format,
            mode,
            case_count,
            unicode_mode,
        })
    }

    /// Returns the requested grammar format.
    pub const fn format(self) -> GrammarFormat {
        self.format
    }

    /// Returns the requested grammar mode.
    pub const fn mode(self) -> GrammarMode {
        self.mode
    }

    /// Returns the requested number of cases.
    pub const fn case_count(self) -> usize {
        self.case_count
    }

    /// Returns the optional Unicode valid-text mode to compose into leaf content.
    pub const fn unicode_mode(self) -> Option<UnicodeMode> {
        self.unicode_mode
    }
}

/// Generated grammar case with a stable zero-based index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarCase {
    case_index: usize,
    text: String,
}

impl GrammarCase {
    /// Returns the stable zero-based case index.
    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    /// Returns the generated valid UTF-8 text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Generates deterministic grammar-aware text cases.
pub fn generate_grammar_cases(
    master_seed: &MasterSeed,
    spec: GrammarCaseSpec,
) -> Result<Vec<GrammarCase>> {
    let context = stream_context(spec);
    let mut stream =
        DeterministicStream::from_seed_with_context(master_seed, DOMAIN_GRAMMAR, context);
    let unicode_fixtures = unicode_fixtures(master_seed, spec)?;
    let mut cases = Vec::with_capacity(spec.case_count);

    for case_index in 0..spec.case_count {
        let leaf = leaf_content(case_index, unicode_fixtures.as_deref(), &mut stream)?;
        let text = render_case(spec.format, spec.mode, case_index, &leaf, &mut stream)?;

        cases.push(GrammarCase { case_index, text });
    }

    Ok(cases)
}

const LEAF_FIXTURES: &[&str] = &[
    "alpha",
    "bravo-42",
    "line edge",
    "token:colon",
    "bracket[leaf]",
    "hash#leaf",
];

const MARKDOWN_VALID_TEMPLATES: &[MarkdownTemplate] = &[
    MarkdownTemplate::HeadingList,
    MarkdownTemplate::LinkAndCode,
    MarkdownTemplate::Table,
];

const MARKDOWN_NEAR_VALID_TEMPLATES: &[MarkdownTemplate] = &[
    MarkdownTemplate::AmbiguousLink,
    MarkdownTemplate::SkippedList,
    MarkdownTemplate::FenceWithTrailingTick,
];

const MARKDOWN_MALFORMED_TEMPLATES: &[MarkdownTemplate] = &[
    MarkdownTemplate::OpenFence,
    MarkdownTemplate::BrokenTable,
    MarkdownTemplate::BrokenLink,
];

const JSON_VALID_TEMPLATES: &[JsonTemplate] = &[
    JsonTemplate::Object,
    JsonTemplate::Array,
    JsonTemplate::Nested,
];

const JSON_NEAR_VALID_TEMPLATES: &[JsonTemplate] = &[
    JsonTemplate::DuplicateKeys,
    JsonTemplate::StringNumber,
    JsonTemplate::EscapedControls,
];

const JSON_MALFORMED_TEMPLATES: &[JsonTemplate] = &[
    JsonTemplate::MissingBrace,
    JsonTemplate::TrailingComma,
    JsonTemplate::UnquotedKey,
];

#[derive(Debug, Clone, Copy)]
enum MarkdownTemplate {
    HeadingList,
    LinkAndCode,
    Table,
    AmbiguousLink,
    SkippedList,
    FenceWithTrailingTick,
    OpenFence,
    BrokenTable,
    BrokenLink,
}

#[derive(Debug, Clone, Copy)]
enum JsonTemplate {
    Object,
    Array,
    Nested,
    DuplicateKeys,
    StringNumber,
    EscapedControls,
    MissingBrace,
    TrailingComma,
    UnquotedKey,
}

fn stream_context(spec: GrammarCaseSpec) -> String {
    let unicode_label = spec
        .unicode_mode
        .map_or("none", corpusforge_unicode::UnicodeMode::label);

    format!(
        "grammar/v1/{}/{}/unicode={unicode_label}",
        spec.format.label(),
        spec.mode.label()
    )
}

fn unicode_fixtures(
    master_seed: &MasterSeed,
    spec: GrammarCaseSpec,
) -> Result<Option<Vec<String>>> {
    let Some(mode) = spec.unicode_mode else {
        return Ok(None);
    };

    let text = generate_valid_text(master_seed, mode, spec.case_count)?;

    Ok(Some(text.lines().map(str::to_owned).collect()))
}

fn leaf_content(
    case_index: usize,
    unicode_fixtures: Option<&[String]>,
    stream: &mut DeterministicStream,
) -> Result<String> {
    let fixture_index = stream.usize_below(LEAF_FIXTURES.len())?;
    let base = LEAF_FIXTURES[fixture_index];

    match unicode_fixtures.and_then(|fixtures| fixtures.get(case_index)) {
        Some(unicode) => Ok(format!("{base}:{unicode}")),
        None => Ok(base.to_owned()),
    }
}

fn render_case(
    format: GrammarFormat,
    mode: GrammarMode,
    case_index: usize,
    leaf: &str,
    stream: &mut DeterministicStream,
) -> Result<String> {
    match format {
        GrammarFormat::Markdown => render_markdown(mode, case_index, leaf, stream),
        GrammarFormat::Json => render_json(mode, case_index, leaf, stream),
    }
}

fn render_markdown(
    mode: GrammarMode,
    case_index: usize,
    leaf: &str,
    stream: &mut DeterministicStream,
) -> Result<String> {
    let templates = match mode {
        GrammarMode::Valid => MARKDOWN_VALID_TEMPLATES,
        GrammarMode::NearValid => MARKDOWN_NEAR_VALID_TEMPLATES,
        GrammarMode::Malformed => MARKDOWN_MALFORMED_TEMPLATES,
    };
    let template = templates[stream.usize_below(templates.len())?];
    let title = format!("Case {case_index}");

    Ok(match template {
        MarkdownTemplate::HeadingList => {
            format!("# {title}\n\n- payload: {leaf}\n- mode: valid\n")
        }
        MarkdownTemplate::LinkAndCode => {
            format!(
                "## {title}\n\nParagraph with `{leaf}` and [stable-link](https://example.invalid/case-{case_index}).\n"
            )
        }
        MarkdownTemplate::Table => {
            format!(
                "| key | value |\n| --- | --- |\n| case | {case_index} |\n| payload | {leaf} |\n"
            )
        }
        MarkdownTemplate::AmbiguousLink => {
            format!("# {title}\n\nParagraph with [edge link](<not-closed) and {leaf}.\n")
        }
        MarkdownTemplate::SkippedList => {
            format!("> {leaf}\n\n1. first\n3. skipped ordinal\n2. repeated shape\n")
        }
        MarkdownTemplate::FenceWithTrailingTick => {
            format!("```json\n{{\"case\":{case_index},\"payload\":\"{leaf}\"}}\n```\n\nTrailing tick `\n")
        }
        MarkdownTemplate::OpenFence => {
            format!("# {title}\n\n```json\n{{\"case\":{case_index},\"payload\":\"{leaf}\"}}\n")
        }
        MarkdownTemplate::BrokenTable => {
            format!("| key | value |\n| --- |\n| payload | {leaf} |\n")
        }
        MarkdownTemplate::BrokenLink => {
            format!("# {title}\n\n[broken link]({leaf}\n\n![missing alt\n")
        }
    })
}

fn render_json(
    mode: GrammarMode,
    case_index: usize,
    leaf: &str,
    stream: &mut DeterministicStream,
) -> Result<String> {
    let templates = match mode {
        GrammarMode::Valid => JSON_VALID_TEMPLATES,
        GrammarMode::NearValid => JSON_NEAR_VALID_TEMPLATES,
        GrammarMode::Malformed => JSON_MALFORMED_TEMPLATES,
    };
    let template = templates[stream.usize_below(templates.len())?];
    let escaped_leaf = escape_json_string(leaf);

    Ok(match template {
        JsonTemplate::Object => {
            format!(
                "{{\"kind\":\"corpusforge\",\"case\":{case_index},\"payload\":\"{escaped_leaf}\"}}"
            )
        }
        JsonTemplate::Array => {
            format!("[{{\"case\":{case_index}}},{{\"payload\":\"{escaped_leaf}\"}},\"valid\"]")
        }
        JsonTemplate::Nested => {
            format!(
                "{{\"case\":{case_index},\"items\":[{{\"text\":\"{escaped_leaf}\"}}],\"ok\":true}}"
            )
        }
        JsonTemplate::DuplicateKeys => {
            format!(
                "{{\"case\":{case_index},\"payload\":\"{escaped_leaf}\",\"payload\":\"shadow\"}}"
            )
        }
        JsonTemplate::StringNumber => {
            format!(
                "{{\"case\":\"000{case_index}\",\"payload\":\"{escaped_leaf}\",\"limit\":\"18446744073709551616\"}}"
            )
        }
        JsonTemplate::EscapedControls => {
            format!(
                "{{\"case\":{case_index},\"payload\":\"{escaped_leaf}\",\"sentinel\":\"line\\nbreak\\tindent\"}}"
            )
        }
        JsonTemplate::MissingBrace => {
            format!("{{\"case\":{case_index},\"payload\":\"{escaped_leaf}\"")
        }
        JsonTemplate::TrailingComma => {
            format!("{{\"case\":{case_index},\"payload\":\"{escaped_leaf}\",}}")
        }
        JsonTemplate::UnquotedKey => {
            format!("{{case:{case_index},\"payload\":\"{escaped_leaf}\"}}")
        }
    })
}

fn escape_json_string(text: &str) -> String {
    let mut escaped = String::new();

    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{00}'..='\u{1F}' => {
                escaped.push_str("\\u00");
                escaped.push(hex_digit((character as u8) >> 4));
                escaped.push(hex_digit(character as u8));
            }
            _ => escaped.push(character),
        }
    }

    escaped
}

fn hex_digit(value: u8) -> char {
    match value & 0x0f {
        0..=9 => char::from(b'0' + (value & 0x0f)),
        10..=15 => char::from(b'a' + ((value & 0x0f) - 10)),
        _ => unreachable!("masked nibble is always in range"),
    }
}

fn stable_labels<T: Copy + Display, const N: usize>(items: &[T; N]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        escape_json_string, generate_grammar_cases, GrammarCaseSpec, GrammarFormat, GrammarMode,
    };
    use corpusforge_core::seed::MasterSeed;
    use corpusforge_core::CorpusForgeError;
    use corpusforge_testkit::TEST_SEED_BYTES;
    use corpusforge_unicode::{generate_valid_text, UnicodeMode};
    use std::str::FromStr;

    const TEST_SEED: MasterSeed = MasterSeed::from_bytes(TEST_SEED_BYTES);

    #[test]
    fn exposes_stable_format_labels_and_order() {
        let labels = GrammarFormat::ALL.map(GrammarFormat::label);

        assert_eq!(labels, ["markdown", "json"]);
    }

    #[test]
    fn parses_and_displays_format_labels() {
        for format in GrammarFormat::ALL {
            let parsed = GrammarFormat::from_str(format.label())
                .unwrap_or_else(|error| panic!("format label should parse: {error}"));

            assert_eq!(parsed, format);
            assert_eq!(parsed.to_string(), format.label());
        }
    }

    #[test]
    fn rejects_unknown_format_label() {
        let error = GrammarFormat::from_str("xml").expect_err("unknown format should fail");

        assert_invalid_argument_contains(&error, "unsupported grammar format");
        assert_invalid_argument_contains(&error, "markdown, json");
    }

    #[test]
    fn exposes_stable_mode_labels_and_order() {
        let labels = GrammarMode::ALL.map(GrammarMode::label);

        assert_eq!(labels, ["valid", "near-valid", "malformed"]);
    }

    #[test]
    fn parses_and_displays_mode_labels() {
        for mode in GrammarMode::ALL {
            let parsed = GrammarMode::from_str(mode.label())
                .unwrap_or_else(|error| panic!("mode label should parse: {error}"));

            assert_eq!(parsed, mode);
            assert_eq!(parsed.to_string(), mode.label());
        }
    }

    #[test]
    fn rejects_unknown_mode_label() {
        let error = GrammarMode::from_str("almost").expect_err("unknown mode should fail");

        assert_invalid_argument_contains(&error, "unsupported grammar mode");
        assert_invalid_argument_contains(&error, "valid, near-valid, malformed");
    }

    #[test]
    fn spec_rejects_zero_case_count() {
        let error = GrammarCaseSpec::new(GrammarFormat::Markdown, GrammarMode::Valid, 0, None)
            .expect_err("zero case count should fail");

        assert_invalid_argument_contains(&error, "non-zero case count");
    }

    #[test]
    fn spec_rejects_invalid_utf8_unicode_composition() {
        let error = GrammarCaseSpec::new(
            GrammarFormat::Json,
            GrammarMode::Valid,
            1,
            Some(UnicodeMode::InvalidUtf8),
        )
        .expect_err("invalid UTF-8 cannot be composed into grammar text");

        assert_invalid_argument_contains(&error, "invalid-utf8");
        assert_invalid_argument_contains(&error, "valid UTF-8 text");
    }

    #[test]
    fn spec_exposes_validated_fields() {
        let spec = GrammarCaseSpec::new(
            GrammarFormat::Json,
            GrammarMode::NearValid,
            7,
            Some(UnicodeMode::Emoji),
        )
        .unwrap_or_else(|error| panic!("spec should be valid: {error}"));

        assert_eq!(spec.format(), GrammarFormat::Json);
        assert_eq!(spec.mode(), GrammarMode::NearValid);
        assert_eq!(spec.case_count(), 7);
        assert_eq!(spec.unicode_mode(), Some(UnicodeMode::Emoji));
    }

    #[test]
    fn generation_is_deterministic_for_same_seed_and_spec() {
        let spec = GrammarCaseSpec::new(
            GrammarFormat::Markdown,
            GrammarMode::NearValid,
            8,
            Some(UnicodeMode::Mixed),
        )
        .unwrap_or_else(|error| panic!("spec should be valid: {error}"));

        let left = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));
        let right = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));

        assert_eq!(left, right);
    }

    #[test]
    fn markdown_generation_matches_stable_exact_output_with_unicode_composition() {
        let spec = GrammarCaseSpec::new(
            GrammarFormat::Markdown,
            GrammarMode::Valid,
            3,
            Some(UnicodeMode::Emoji),
        )
        .unwrap_or_else(|error| panic!("spec should be valid: {error}"));
        let cases = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));
        let texts = case_texts(&cases);

        assert_eq!(
            texts,
            [
                "## Case 0\n\nParagraph with `line edge:\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}` and [stable-link](https://example.invalid/case-0).\n",
                "| key | value |\n| --- | --- |\n| case | 1 |\n| payload | token:colon:\u{1f44d}\u{1f3fd} |\n",
                "# Case 2\n\n- payload: bracket[leaf]:5\u{fe0f}\u{20e3}\n- mode: valid\n",
            ]
        );
    }

    #[test]
    fn json_generation_matches_stable_exact_output() {
        let spec = GrammarCaseSpec::new(GrammarFormat::Json, GrammarMode::Valid, 6, None)
            .unwrap_or_else(|error| panic!("spec should be valid: {error}"));
        let cases = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));
        let texts = case_texts(&cases);

        assert_eq!(
            texts,
            [
                "{\"case\":0,\"items\":[{\"text\":\"line edge\"}],\"ok\":true}",
                "{\"kind\":\"corpusforge\",\"case\":1,\"payload\":\"token:colon\"}",
                "{\"case\":2,\"items\":[{\"text\":\"alpha\"}],\"ok\":true}",
                "[{\"case\":3},{\"payload\":\"alpha\"},\"valid\"]",
                "{\"kind\":\"corpusforge\",\"case\":4,\"payload\":\"line edge\"}",
                "{\"case\":5,\"items\":[{\"text\":\"hash#leaf\"}],\"ok\":true}",
            ]
        );
    }

    #[test]
    fn generated_cases_use_stable_zero_based_indexes() {
        let spec = GrammarCaseSpec::new(GrammarFormat::Json, GrammarMode::Valid, 4, None)
            .unwrap_or_else(|error| panic!("spec should be valid: {error}"));
        let cases = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));

        let indexes = cases
            .iter()
            .map(super::GrammarCase::case_index)
            .collect::<Vec<_>>();

        assert_eq!(indexes, [0, 1, 2, 3]);
        assert!(cases.iter().all(|case| !case.text().is_empty()));
    }

    #[test]
    fn different_formats_and_modes_differ_where_practical() {
        let markdown = generate_for(GrammarFormat::Markdown, GrammarMode::Valid);
        let json = generate_for(GrammarFormat::Json, GrammarMode::Valid);
        let near_valid_json = generate_for(GrammarFormat::Json, GrammarMode::NearValid);
        let malformed_json = generate_for(GrammarFormat::Json, GrammarMode::Malformed);

        assert_ne!(markdown, json);
        assert_ne!(json, near_valid_json);
        assert_ne!(near_valid_json, malformed_json);
    }

    #[test]
    fn valid_json_cases_are_syntactically_valid() {
        let spec = GrammarCaseSpec::new(GrammarFormat::Json, GrammarMode::Valid, 16, None)
            .unwrap_or_else(|error| panic!("spec should be valid: {error}"));
        let cases = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));

        for case in cases {
            assert_valid_json(case.text());
        }
    }

    #[test]
    fn unicode_composition_injects_existing_valid_text_fixture_into_markdown() {
        let spec = GrammarCaseSpec::new(
            GrammarFormat::Markdown,
            GrammarMode::Valid,
            3,
            Some(UnicodeMode::Emoji),
        )
        .unwrap_or_else(|error| panic!("spec should be valid: {error}"));
        let cases = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));
        let unicode = generate_valid_text(&TEST_SEED, UnicodeMode::Emoji, 3)
            .unwrap_or_else(|error| panic!("Unicode generation should succeed: {error}"));

        for (case, fixture) in cases.iter().zip(unicode.lines()) {
            assert!(
                case.text().contains(fixture),
                "case text should include generated Unicode fixture {fixture:?}: {:?}",
                case.text()
            );
        }
    }

    #[test]
    fn unicode_composition_injects_existing_valid_text_fixture_into_json() {
        let spec = GrammarCaseSpec::new(
            GrammarFormat::Json,
            GrammarMode::Valid,
            3,
            Some(UnicodeMode::Grapheme),
        )
        .unwrap_or_else(|error| panic!("spec should be valid: {error}"));
        let cases = generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"));
        let unicode = generate_valid_text(&TEST_SEED, UnicodeMode::Grapheme, 3)
            .unwrap_or_else(|error| panic!("Unicode generation should succeed: {error}"));

        for (case, fixture) in cases.iter().zip(unicode.lines()) {
            assert!(
                case.text().contains(&escape_json_string(fixture)),
                "JSON text should include escaped generated Unicode fixture {fixture:?}: {:?}",
                case.text()
            );
            assert_valid_json(case.text());
        }
    }

    #[test]
    fn json_string_escaping_handles_quotes_backslashes_and_controls() {
        let escaped = escape_json_string("quote\" slash\\ newline\n tab\t bell\u{07}");

        assert_eq!(escaped, "quote\\\" slash\\\\ newline\\n tab\\t bell\\u0007");
    }

    fn generate_for(format: GrammarFormat, mode: GrammarMode) -> Vec<String> {
        let spec = GrammarCaseSpec::new(format, mode, 4, None)
            .unwrap_or_else(|error| panic!("spec should be valid: {error}"));

        generate_grammar_cases(&TEST_SEED, spec)
            .unwrap_or_else(|error| panic!("generation should succeed: {error}"))
            .into_iter()
            .map(|case| case.text().to_owned())
            .collect()
    }

    fn case_texts(cases: &[super::GrammarCase]) -> Vec<&str> {
        cases.iter().map(super::GrammarCase::text).collect()
    }

    fn assert_valid_json(text: &str) {
        let mut parser = JsonParser::new(text);
        parser
            .parse_value()
            .unwrap_or_else(|error| panic!("invalid JSON {text:?}: {error}"));
        parser
            .finish()
            .unwrap_or_else(|error| panic!("invalid JSON {text:?}: {error}"));
    }

    struct JsonParser<'a> {
        text: &'a str,
        offset: usize,
    }

    impl<'a> JsonParser<'a> {
        fn new(text: &'a str) -> Self {
            Self { text, offset: 0 }
        }

        fn finish(&mut self) -> std::result::Result<(), String> {
            self.skip_whitespace();

            if self.offset == self.text.len() {
                Ok(())
            } else {
                Err(format!("trailing input at byte {}", self.offset))
            }
        }

        fn parse_value(&mut self) -> std::result::Result<(), String> {
            self.skip_whitespace();

            match self.peek() {
                Some('{') => self.parse_object(),
                Some('[') => self.parse_array(),
                Some('"') => self.parse_string().map(drop),
                Some('t') => self.consume_literal("true"),
                Some('f') => self.consume_literal("false"),
                Some('n') => self.consume_literal("null"),
                Some('-' | '0'..='9') => self.parse_number(),
                Some(character) => Err(format!(
                    "unexpected character {character:?} at byte {}",
                    self.offset
                )),
                None => Err("unexpected end of input".to_owned()),
            }
        }

        fn parse_object(&mut self) -> std::result::Result<(), String> {
            self.expect('{')?;
            self.skip_whitespace();

            if self.consume_if('}') {
                return Ok(());
            }

            loop {
                self.skip_whitespace();
                self.parse_string()?;
                self.skip_whitespace();
                self.expect(':')?;
                self.parse_value()?;
                self.skip_whitespace();

                if self.consume_if('}') {
                    return Ok(());
                }

                self.expect(',')?;
            }
        }

        fn parse_array(&mut self) -> std::result::Result<(), String> {
            self.expect('[')?;
            self.skip_whitespace();

            if self.consume_if(']') {
                return Ok(());
            }

            loop {
                self.parse_value()?;
                self.skip_whitespace();

                if self.consume_if(']') {
                    return Ok(());
                }

                self.expect(',')?;
            }
        }

        fn parse_string(&mut self) -> std::result::Result<String, String> {
            self.expect('"')?;
            let mut value = String::new();

            loop {
                let Some(character) = self.next() else {
                    return Err("unterminated string".to_owned());
                };

                match character {
                    '"' => return Ok(value),
                    '\\' => value.push(self.parse_escape()?),
                    '\u{00}'..='\u{1f}' => {
                        return Err(format!(
                            "unescaped control character at byte {}",
                            self.offset
                        ));
                    }
                    _ => value.push(character),
                }
            }
        }

        fn parse_escape(&mut self) -> std::result::Result<char, String> {
            match self.next() {
                Some(character @ ('"' | '\\' | '/')) => Ok(character),
                Some('b') => Ok('\u{08}'),
                Some('f') => Ok('\u{0c}'),
                Some('n') => Ok('\n'),
                Some('r') => Ok('\r'),
                Some('t') => Ok('\t'),
                Some('u') => self.parse_unicode_escape(),
                Some(character) => Err(format!("invalid escape {character:?}")),
                None => Err("unterminated escape".to_owned()),
            }
        }

        fn parse_unicode_escape(&mut self) -> std::result::Result<char, String> {
            let mut value = 0_u32;

            for _ in 0..4 {
                let Some(character) = self.next() else {
                    return Err("unterminated unicode escape".to_owned());
                };
                let Some(digit) = character.to_digit(16) else {
                    return Err(format!("invalid unicode escape digit {character:?}"));
                };
                value = (value << 4) | digit;
            }

            char::from_u32(value).ok_or_else(|| format!("invalid unicode scalar U+{value:04X}"))
        }

        fn parse_number(&mut self) -> std::result::Result<(), String> {
            self.consume_if('-');

            match self.peek() {
                Some('0') => {
                    self.next();
                }
                Some('1'..='9') => {
                    self.next();
                    while matches!(self.peek(), Some('0'..='9')) {
                        self.next();
                    }
                }
                _ => return Err(format!("invalid number at byte {}", self.offset)),
            }

            if self.consume_if('.') {
                self.consume_digits("fraction")?;
            }

            if matches!(self.peek(), Some('e' | 'E')) {
                self.next();
                let _has_sign = self.consume_if('+') || self.consume_if('-');
                self.consume_digits("exponent")?;
            }

            Ok(())
        }

        fn consume_digits(&mut self, name: &str) -> std::result::Result<(), String> {
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(format!("missing {name} digit at byte {}", self.offset));
            }

            while matches!(self.peek(), Some('0'..='9')) {
                self.next();
            }

            Ok(())
        }

        fn consume_literal(&mut self, literal: &str) -> std::result::Result<(), String> {
            if self.text[self.offset..].starts_with(literal) {
                self.offset += literal.len();
                Ok(())
            } else {
                Err(format!("expected {literal:?} at byte {}", self.offset))
            }
        }

        fn expect(&mut self, expected: char) -> std::result::Result<(), String> {
            match self.next() {
                Some(actual) if actual == expected => Ok(()),
                Some(actual) => Err(format!(
                    "expected {expected:?}, found {actual:?} at byte {}",
                    self.offset
                )),
                None => Err(format!("expected {expected:?}, found end of input")),
            }
        }

        fn consume_if(&mut self, expected: char) -> bool {
            if self.peek() == Some(expected) {
                self.next();
                true
            } else {
                false
            }
        }

        fn skip_whitespace(&mut self) {
            while matches!(self.peek(), Some(' ' | '\n' | '\r' | '\t')) {
                self.next();
            }
        }

        fn peek(&self) -> Option<char> {
            self.text[self.offset..].chars().next()
        }

        fn next(&mut self) -> Option<char> {
            let character = self.peek()?;
            self.offset += character.len_utf8();
            Some(character)
        }
    }

    fn assert_invalid_argument_contains(error: &CorpusForgeError, expected: &str) {
        assert_eq!(error.category(), "invalid_argument");
        assert!(
            error.to_string().contains(expected),
            "expected '{error}' to contain '{expected}'"
        );
    }
}
