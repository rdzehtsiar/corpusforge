// SPDX-License-Identifier: Apache-2.0

use corpusforge_cli::{run, write_outcome};

fn main() {
    let outcome = run(std::env::args_os());
    let exit_code = write_outcome(outcome, &mut std::io::stdout(), &mut std::io::stderr());

    std::process::exit(exit_code);
}
