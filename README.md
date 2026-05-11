# CorpusForge

CorpusForge is a planned offline, deterministic corpus compiler for hostile text. It is intended for engineers who need reproducible inputs that stress tokenizers, parsers, renderers, compression behavior, Unicode handling, and text preprocessing pipelines.

The project is not an AI writing tool, a local language model, or a generic lorem ipsum generator. Its goal is engineering reliability: generate adversarial text and byte corpora that can be reproduced, minimized, and turned into regression fixtures.

## Vision

CorpusForge is meant to make hostile text testing practical in local development and CI.

The product direction is:

- deterministic generation from explicit seeds and profile metadata
- Unicode-aware tokenizer and parser torture testing
- reproducible failing samples and byte ranges
- shrinking/minimization of failure cases
- transparent profile formats that can be inspected and verified
- offline-first operation with no telemetry or cloud dependency
- clear documentation of what is supported, partial, unstable, or intentionally unsupported

## Current State

CorpusForge is at the very beginning of development.

This repository currently contains planning documentation and project metadata. There is not yet an implemented application, stable CLI, crate layout, package manager setup, or test harness. Commands, file formats, architecture, and compatibility claims should be treated as planned until implemented and covered by tests.

Do not rely on CorpusForge for production workflows yet.

## Intended Users

CorpusForge is for engineers who need to answer practical questions such as:

- whether a tokenizer handles hostile Unicode and byte sequences correctly
- why a parser or renderer crashes on rare text edge cases
- how to reproduce a text-processing failure from a seed and profile
- how to minimize a failing input into a stable regression fixture
- how ingestion, embedding, RAG, or preprocessing pipelines behave with adversarial text
- how to run deterministic text stress tests in CI without network access

## Planned Scope

Initial development is focused on:

- seedable corpus profiles
- deterministic text and byte generation
- Unicode adversarial modes
- profile inspection and verification
- reproducible replay
- shrinking/minimization workflows
- CI-friendly reports

Later work may add grammar-aware generation for formats such as Markdown and JSON once the core deterministic and Unicode-focused workflows are solid.

## Non-Goals

CorpusForge is not intended to be:

- a generic AI text generator
- a transformer runtime
- a hosted service
- a telemetry-backed product
- a tool that requires a cloud account
- a replacement for format-specific conformance suites
- a guarantee of parser or tokenizer correctness without evidence

## Principles

- offline by default
- no telemetry by default
- deterministic output where practical
- reproducible profiles, generated corpora, replay ranges, and minimized cases
- compatibility and reliability claims backed by tests
- explicit unsupported and partial behavior
- stable, inspectable report formats
- cross-platform development and CI friendliness

## Development

Implementation has not started. When it does, the current plan favors a Rust CLI-first workspace with deterministic tests and a single static binary as a distribution goal.

Expected local checks will be documented once the workspace exists. Early Rust checks are expected to include commands such as:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

These checks should stay offline and deterministic.

## License

Licensed under the Apache License, Version 2.0.
See [LICENSE](./LICENSE.txt).
