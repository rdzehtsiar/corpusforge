# CorpusForge

[![Tests](https://github.com/rdzehtsiar/corpusforge/actions/workflows/tests.yml/badge.svg)](https://github.com/rdzehtsiar/corpusforge/actions/workflows/tests.yml)
[![codecov](https://codecov.io/gh/rdzehtsiar/corpusforge/graph/badge.svg)](https://codecov.io/gh/rdzehtsiar/corpusforge)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=rdzehtsiar_corpusforge&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=rdzehtsiar_corpusforge)

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

CorpusForge is at an early implementation stage.

This repository now contains a Rust workspace, shared error types, deterministic seed and stream primitives, a CLI skeleton, Milestone 3 `.cff` v0 profile support, and Milestone 6 built-in tokenizer Unicode workflows. The `corpusforge` binary can print top-level and command-specific help, and `.cff` v0 profile build, read, inspect, and verify workflows exist for deterministic fixture profiles.

The `corpusforge-unicode` crate includes Milestone 4 library APIs for deterministic, fixture-based Unicode adversarial generation. The CLI also exposes these fixture-based tokenizer modes through `corpusforge gen --unicode ...` and `corpusforge ci tokenizer`: `grapheme`, `bidi`, `zero-width`, `emoji`, `normalization`, `mixed`, and `invalid-utf8`.

Unicode output boundaries are intentionally separate. Valid-text generation returns UTF-8 text and rejects `invalid-utf8`. Raw-byte generation returns bytes and is the only supported path for `invalid-utf8` cases. The current implementation samples from built-in fixtures with deterministic streams; it is not a broad Unicode or tokenizer compatibility guarantee.

N-gram training and profile-backed generation are implemented as a byte-level bigram MVP. `corpusforge ci tokenizer` can run an external stdin harness against built-in tokenizer Unicode samples and write a stable JSON report. Shrinking, replay metadata, broader CI integrations, packaging, and release automation are not implemented yet.

Profile format support is limited to unstable `.cff` v0 behavior with no cross-version compatibility guarantee. Broader deterministic output guarantees, compatibility claims, and generation behavior should be treated as planned until implemented and covered by tests.

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

Development currently uses a Rust CLI-first workspace with deterministic tests and a single static binary as a distribution goal.

Local checks:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p corpusforge-cli -- --help
```

These checks should stay offline and deterministic.

Project documentation:

- [Architecture](./docs/architecture.md)
- [Determinism](./docs/determinism.md)
- [Tokenizer workflow demo](./docs/tokenizer-workflow.md)
- [Roadmap](./docs/roadmap.md)
- [Agent workflow](./docs/agent-workflow.md)
- [Contributing](./CONTRIBUTING.md)

## License

Licensed under the Apache License, Version 2.0.
See [LICENSE](./LICENSE.txt).
