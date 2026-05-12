# Architecture

## Product Definition

CorpusForge is an offline deterministic corpus compiler for hostile text testing. It is intended to help engineers create reproducible inputs that stress tokenizers, parsers, renderers, compression pipelines, Unicode handling, and preprocessing systems.

The first product focus is Unicode-aware tokenizer and parser torture testing with reproducible failing samples and future shrinking support.

CorpusForge is not an AI writing tool, a local language model, a hosted service, a telemetry product, or a generic synthetic content engine.

## Current Shape

The repository has moved beyond the initial foundation, but the main corpus generation pipeline is still incomplete. The Rust workspace, CLI skeleton, deterministic core primitives, Milestone 3 `.cff` v0 profile workflows, and initial Milestone 4 Unicode fixture APIs exist.

Implemented now:

- Rust workspace with placeholder crates.
- `corpusforge-cli` package and `corpusforge` binary.
- CLI help and version output.
- Shared project error categories, seed parsing, domain-separated deterministic streams, and integer sampling in `corpusforge-core`.
- `.cff` v0 profile reader, writer, verifier, versioning, and hashing logic.
- Deterministic fixture profile compilation for `.cff` v0 profile packs.
- CLI profile build, inspect, and verify behavior for implemented fixture profile workflows.
- `corpusforge-unicode` mode and output-boundary APIs.
- Fixture-based deterministic Unicode adversarial generation through `generate_valid_text` and `generate_raw_bytes`.
- Unicode mode labels: `grapheme`, `bidi`, `zero-width`, `emoji`, `normalization`, `mixed`, and `invalid-utf8`.
- Valid-text versus raw-byte boundary validation, including rejection of `invalid-utf8` for valid-text output.

Not implemented yet:

- CLI corpus generation
- profile-driven Unicode corpus generation
- n-gram training or sampling
- shrinking or replay behavior
- CI report formats
- static packaging or release automation

## High-Level Architecture

```text
Local inputs / built-in profiles / future grammars
                  |
                  v
            Profile compiler
                  |
                  v
            .cff profile pack
                  |
        +---------+---------+
        |                   |
        v                   v
  Corpus generator       Shrinker
        |                   |
        v                   v
 Corpus output       Minimal reproducer
        |                   |
        v                   v
 Local tests / CI     Regression fixture
```

The diagram describes the planned architecture. Milestone 3 implements deterministic `.cff` v0 fixture profile build, read, inspect, verify, and hashing pieces. Milestone 4 currently adds library-level Unicode fixture generation, but the CLI generator, shrinker, replay, and CI reporting stages remain planned.

## Crate Responsibilities

- `corpusforge-cli`: command-line parsing, help text, exit behavior, and orchestration. It should not contain core generation logic.
- `corpusforge-core`: shared errors, result types, seed handling, deterministic stream primitives, and shared domain types.
- `corpusforge-cff`: implemented `.cff` v0 profile pack reader, writer, verifier, versioning, and hashing logic.
- `corpusforge-profile`: deterministic fixture profile compilation into `.cff` v0 profile packs, with broader profile compilation still planned.
- `corpusforge-unicode`: implemented fixture-based Unicode adversarial valid-text and raw-byte APIs, including mode/output validation for grapheme, bidi, zero-width, emoji, normalization, mixed, and invalid-utf8 cases. This crate does not yet provide broad Unicode mutation, confusable generation, or CLI-integrated corpus generation.
- `corpusforge-ngram`: future deterministic weighted n-gram profile building and sampling.
- `corpusforge-shrink`: future reducer and minimizer logic independent from specific parsers or tokenizers.
- `corpusforge-testkit`: shared deterministic test utilities and fixture helpers.

## Non-Goals

Initial development does not include:

- hosted services or cloud accounts
- telemetry, analytics, crash upload, or update checks
- runtime ML or transformer behavior in the default binary
- broad format support across every parser ecosystem
- production corpus management or long-term storage
- nondeterministic sampling that cannot be reproduced
- compatibility claims without tests naming the verified behavior

## Current Limitations

The current CLI is intentionally narrow. Help text keeps planned command behavior visible, and implemented profile commands can build, inspect, and verify `.cff` v0 fixture profile packs. Planned commands still do not generate, shrink, replay, or report on corpora yet.

The Unicode crate can generate deterministic fixture output through direct Rust APIs, but that behavior is not connected to the CLI generation flow. `invalid-utf8` is intentionally limited to raw-byte output; valid-text output rejects it because it may produce bytes that cannot decode as UTF-8.

The v0 architecture is still subject to change. Public command names should be treated as early project direction. The `.cff` v0 format is implemented for current fixtures, but it is unstable and has no cross-version compatibility guarantee. Corpus output behavior is not stable until later milestones define and test CLI integration, report formats, and broader generation workflows.
