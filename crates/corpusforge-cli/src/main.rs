// SPDX-License-Identifier: Apache-2.0

use corpusforge_cli::run_to_writers;

fn main() {
    let exit_code = run_to_writers(
        std::env::args_os(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );

    std::process::exit(exit_code);
}
