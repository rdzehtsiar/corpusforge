// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = Vec::new();
    if let Err(error) = io::stdin().read_to_end(&mut input) {
        eprintln!("failed to read stdin: {error}");
        return ExitCode::from(2);
    }

    match std::str::from_utf8(&input) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("invalid UTF-8 at byte {}: {error}", error.valid_up_to());
            ExitCode::from(1)
        }
    }
}
