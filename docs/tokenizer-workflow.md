# Tokenizer Workflow Demo

CorpusForge Milestone 6 includes fixture-based tokenizer workflows for built-in Unicode modes. These commands are deterministic for the same CorpusForge version, seed, mode, output kind, and case count, but they are not a broad tokenizer correctness or Unicode conformance suite.

The demo harness in `examples/reject_invalid_utf8.rs` reads bytes from stdin and exits with a nonzero status when the input is not valid UTF-8. It has no dependencies and is intended to show how `corpusforge ci tokenizer` records a failing stdin harness run.

## Build the Demo Harness

From the repository root:

```powershell
New-Item -ItemType Directory -Force target
rustc examples\reject_invalid_utf8.rs -o target\reject-invalid-utf8-demo.exe
```

On Unix-like shells:

```sh
mkdir -p target
rustc examples/reject_invalid_utf8.rs -o target/reject-invalid-utf8-demo
```

## Generate Tokenizer Unicode Samples

Generate built-in valid-text tokenizer samples:

```powershell
cargo run -p corpusforge-cli -- gen --unicode mixed --output-kind valid-text --cases 8 --seed 1337 --out target\tokenizer-valid.txt
```

Generate raw-byte samples that may include invalid UTF-8:

```powershell
cargo run -p corpusforge-cli -- gen --unicode invalid-utf8 --output-kind raw-bytes --cases 4 --seed 1337 --out target\tokenizer-invalid.bin
```

`invalid-utf8` is only supported with `--output-kind raw-bytes`; valid-text generation rejects that mode.

## Run CI Against the Harness

Run the stdin harness once per generated tokenizer case and write a stable JSON report:

```powershell
cargo run -p corpusforge-cli -- ci tokenizer --unicode invalid-utf8 --output-kind raw-bytes --cases 4 --seed 1337 --command target\reject-invalid-utf8-demo.exe --report-out target\tokenizer-report.json
```

On Unix-like shells:

```sh
cargo run -p corpusforge-cli -- ci tokenizer --unicode invalid-utf8 --output-kind raw-bytes --cases 4 --seed 1337 --command target/reject-invalid-utf8-demo --report-out target/tokenizer-report.json
```

This command is expected to fail because the demo harness rejects invalid UTF-8. The report is written on both passing and failing runs.

## Reproduce and Inspect Offline

The failing bytes can be regenerated offline with the same seed and flags:

```powershell
cargo run -p corpusforge-cli -- gen --unicode invalid-utf8 --output-kind raw-bytes --cases 4 --seed 1337 --out target\tokenizer-invalid.bin
cmd /c "target\reject-invalid-utf8-demo.exe < target\tokenizer-invalid.bin"
Get-Content target\tokenizer-report.json
```

On Unix-like shells:

```sh
cargo run -p corpusforge-cli -- gen --unicode invalid-utf8 --output-kind raw-bytes --cases 4 --seed 1337 --out target/tokenizer-invalid.bin
./target/reject-invalid-utf8-demo < target/tokenizer-invalid.bin
cat target/tokenizer-report.json
```

The JSON report includes the command, seed, Unicode mode, output kind, case count, harness command, aggregate result, and first failure summary. `profile_hash` is currently `null` for this built-in tokenizer workflow.

## Current Limitations

Milestone 6 tokenizer workflows use built-in fixture-based Unicode samples. They support stdin harness execution and stable JSON reports for these tokenizer cases, but they do not implement shrinking, replay metadata, broad CI integrations, or broad parser/tokenizer compatibility claims.
