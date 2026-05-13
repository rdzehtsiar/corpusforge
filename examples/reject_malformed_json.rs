// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read stdin as UTF-8 text: {error}");
        return ExitCode::from(2);
    }

    match find_json_issue(&input) {
        Some(issue) => {
            eprintln!("{issue}");
            ExitCode::from(1)
        }
        None => ExitCode::SUCCESS,
    }
}

fn find_json_issue(input: &str) -> Option<String> {
    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if !starts_and_ends_like_json_value(trimmed) {
            return Some(format!(
                "line {line_number} does not start and end like a JSON object or array"
            ));
        }

        if let Err(error) = check_jsonish_structure(trimmed) {
            return Some(format!("line {line_number}: {error}"));
        }
    }

    None
}

fn starts_and_ends_like_json_value(line: &str) -> bool {
    (line.starts_with('{') && line.ends_with('}')) || (line.starts_with('[') && line.ends_with(']'))
}

fn check_jsonish_structure(line: &str) -> Result<(), String> {
    let mut stack = Vec::new();
    let mut string_state = StringState::Outside;
    let mut previous = PreviousToken::Start;

    for current in line.chars() {
        if string_state.is_inside() {
            if string_state.accept(current) == StringEvent::Closed {
                previous = PreviousToken::Value;
            }
            continue;
        }

        match current {
            '"' => string_state = StringState::Inside,
            '{' => {
                stack.push(JsonishDelimiter::Object);
                previous = PreviousToken::Open;
            }
            '[' => {
                stack.push(JsonishDelimiter::Array);
                previous = PreviousToken::Open;
            }
            '}' => {
                reject_bad_closing_position(previous)?;
                require_delimiter(&mut stack, JsonishDelimiter::Object)?;
                previous = PreviousToken::Value;
            }
            ']' => {
                reject_bad_closing_position(previous)?;
                require_delimiter(&mut stack, JsonishDelimiter::Array)?;
                previous = PreviousToken::Value;
            }
            ',' => {
                require_value_before_comma(previous)?;
                previous = PreviousToken::Comma;
            }
            ':' => {
                require_object_colon(&stack, previous)?;
                previous = PreviousToken::Colon;
            }
            current if current.is_whitespace() => {}
            _ => previous = previous.after_bare_value(),
        }
    }

    if string_state.is_inside() {
        return Err("string literal is not closed".to_string());
    }

    if !stack.is_empty() {
        return Err("object or array delimiter is not closed".to_string());
    }

    Ok(())
}

fn require_value_before_comma(previous: PreviousToken) -> Result<(), String> {
    if matches!(
        previous,
        PreviousToken::Start | PreviousToken::Open | PreviousToken::Comma | PreviousToken::Colon
    ) {
        Err("comma is not preceded by a value".to_string())
    } else {
        Ok(())
    }
}

fn require_object_colon(
    stack: &[JsonishDelimiter],
    previous: PreviousToken,
) -> Result<(), String> {
    if !stack.last().is_some_and(|value| *value == JsonishDelimiter::Object) {
        return Err("colon appears outside an object".to_string());
    }

    if !matches!(previous, PreviousToken::Value) {
        return Err("colon is not preceded by a key-like string".to_string());
    }

    Ok(())
}

fn reject_bad_closing_position(previous: PreviousToken) -> Result<(), String> {
    if matches!(previous, PreviousToken::Comma | PreviousToken::Colon) {
        Err("closing delimiter follows a comma or colon".to_string())
    } else {
        Ok(())
    }
}

fn require_delimiter(
    stack: &mut Vec<JsonishDelimiter>,
    expected: JsonishDelimiter,
) -> Result<(), String> {
    match stack.pop() {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err("closing delimiter does not match opening delimiter".to_string()),
        None => Err("closing delimiter has no matching opening delimiter".to_string()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JsonishDelimiter {
    Object,
    Array,
}

#[derive(Clone, Copy, Debug)]
enum PreviousToken {
    Start,
    Open,
    Value,
    Comma,
    Colon,
}

impl PreviousToken {
    const fn after_bare_value(self) -> Self {
        match self {
            Self::Colon | Self::Open | Self::Comma => Self::Value,
            _ => self,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringState {
    Outside,
    Inside,
    Escaped,
}

impl StringState {
    const fn is_inside(self) -> bool {
        !matches!(self, Self::Outside)
    }

    fn accept(&mut self, current: char) -> StringEvent {
        *self = match (*self, current) {
            (Self::Escaped, _) => Self::Inside,
            (Self::Inside, '\\') => Self::Escaped,
            (Self::Inside, '"') => {
                return self.close();
            }
            (state, _) => state,
        };
        StringEvent::Open
    }

    fn close(&mut self) -> StringEvent {
        *self = Self::Outside;
        StringEvent::Closed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringEvent {
    Open,
    Closed,
}
