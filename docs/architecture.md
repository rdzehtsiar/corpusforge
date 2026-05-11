# Architecture

## Product Definition

CorpusForge is an offline deterministic corpus compiler for hostile text testing. It is intended to help engineers create reproducible inputs that stress tokenizers, parsers, renderers, compression pipelines, Unicode handling, and preprocessing systems.

The first product focus is Unicode-aware tokenizer and parser torture testing with reproducible failing samples and future shrinking support.

CorpusForge is not an AI writing tool, a local language model, a hosted service, a telemetry product, or a generic synthetic content engine.

## Current Shape

Milestone 1 establishes the repository foundation. The Rust workspace and CLI skeleton exist, but the core product behavior is not implemented yet.

Implemented now:

- Rust workspace with placeholder crates.
- `corpusforge-cli` package and `corpusforge` binary.
- CLI help and version output.
- Placeholder command execution that returns `NotImplemented`.
- Shared project error categories in `corpusforge-core`.

Not implemented yet:

- corpus generation
- `.cff` profile serialization, parsing, verification, or hashing
- Unicode mutation or invalid-byte generation
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

The diagram describes the planned architecture. Milestone 1 only provides the workspace, placeholder crates, and CLI surface needed to build this incrementally.

## Crate Responsibilities

- `corpusforge-cli`: command-line parsing, help text, exit behavior, and orchestration. It should not contain core generation logic.
- `corpusforge-core`: shared errors, result types, seed handling, deterministic stream primitives, and shared domain types.
- `corpusforge-cff`: future `.cff` profile pack reader, writer, verifier, versioning, and hashing logic.
- `corpusforge-profile`: future profile compilation from local inputs into profile packs.
- `corpusforge-unicode`: future Unicode adversarial layers, including normalization, bidi, zero-width characters, emoji sequences, confusables, and byte-mode boundaries.
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

## Milestone 1 Limitations

The current CLI is intentionally shallow. Help text exists so future command behavior has a stable entry point, but executing planned commands does not generate, shrink, replay, verify, or report on corpora yet.

The v0 architecture is still subject to change. Public command names should be treated as early project direction, while file formats and deterministic output behavior are not stable until later milestones define and test them.
