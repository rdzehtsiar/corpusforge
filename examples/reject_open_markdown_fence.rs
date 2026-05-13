// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read stdin as UTF-8 text: {error}");
        return ExitCode::from(2);
    }

    match find_markdown_issue(&input) {
        Some(issue) => {
            eprintln!("{issue}");
            ExitCode::from(1)
        }
        None => ExitCode::SUCCESS,
    }
}

fn find_markdown_issue(input: &str) -> Option<String> {
    if let Some(line) = find_unclosed_fence(input) {
        return Some(format!("unclosed fenced code block opened on line {line}"));
    }

    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if has_open_inline_link(line) {
            return Some(format!(
                "inline Markdown link or image appears unclosed on line {line_number}"
            ));
        }
    }

    None
}

fn find_unclosed_fence(input: &str) -> Option<usize> {
    let mut open_fence: Option<(FenceKind, usize)> = None;

    for (line_index, line) in input.lines().enumerate() {
        let Some(fence) = FenceKind::from_line(line) else {
            continue;
        };

        match open_fence {
            Some((open, _)) if open == fence => open_fence = None,
            None => open_fence = Some((fence, line_index + 1)),
            _ => {}
        }
    }

    open_fence.map(|(_, line)| line)
}

fn has_open_inline_link(line: &str) -> bool {
    let mut open_brackets = 0usize;
    let mut chars = line.chars().peekable();

    while let Some(current) = chars.next() {
        match current {
            '\\' => {
                chars.next();
            }
            '[' => open_brackets += 1,
            ']' => {
                open_brackets = open_brackets.saturating_sub(1);
                if chars.peek() == Some(&'(') {
                    chars.next();
                    if !consume_until_closing_paren(&mut chars) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }

    open_brackets > 0
}

fn consume_until_closing_paren<I>(chars: &mut I) -> bool
where
    I: Iterator<Item = char>,
{
    for current in chars {
        if current == ')' {
            return true;
        }
    }

    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FenceKind {
    Backtick,
    Tilde,
}

impl FenceKind {
    fn from_line(line: &str) -> Option<Self> {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            Some(Self::Backtick)
        } else if trimmed.starts_with("~~~") {
            Some(Self::Tilde)
        } else {
            None
        }
    }
}
