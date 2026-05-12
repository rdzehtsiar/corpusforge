# Architecture

## Product Definition

CorpusForge is an offline deterministic corpus compiler for hostile text testing. It is intended to help engineers create reproducible inputs that stress tokenizers, parsers, renderers, compression pipelines, Unicode handling, and preprocessing systems.

The first product focus is Unicode-aware tokenizer and parser torture testing with reproducible failing samples, byte-level shrinking, and deterministic replay for implemented profile-backed generation.

CorpusForge is not an AI writing tool, a local language model, a hosted service, a telemetry product, or a generic synthetic content engine.

## Current Shape

The repository has moved beyond the initial foundation, but the main corpus generation pipeline is still incomplete. The Rust workspace, deterministic core primitives, Milestone 3 `.cff` v0 profile workflows, Milestone 4 Unicode fixture APIs, Milestone 5 byte-level n-gram generation, Milestone 6 tokenizer workflow, and narrow Milestone 7 shrink/replay MVP exist.

Implemented now:

- Rust workspace with focused crates.
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
- Byte-level bigram n-gram profile generation from `.cff` profiles with embedded models.
- Built-in tokenizer CI workflow with stdin harness execution and stable JSON reports.
- Byte-level shrink that invokes a predicate directly, writes candidate bytes to stdin, preserves reproducible nonzero exits or consistent timeouts, and rejects flaky predicates.
- Profile-backed replay from a `.cff` profile, seed or seed file, and half-open byte range.
- Stable shrink and replay metadata JSON without timestamps.

Not implemented yet:

- profile-driven Unicode corpus generation
- Unicode-aware or structure-aware shrinking
- replay from saved metadata files
- broad CI report formats beyond the tokenizer workflow
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

The diagram describes the planned architecture. Current milestones implement deterministic `.cff` v0 fixture profile build, read, inspect, verify, hashing, fixture-based Unicode generation, byte-level n-gram generation, a tokenizer stdin harness workflow, byte-level shrink, and profile-backed replay. Grammar-aware generation, broad CI integration, packaging, and release automation remain planned.

## Crate Responsibilities

- `corpusforge-cli`: command-line parsing, help text, exit behavior, and orchestration. It should not contain core generation logic.
- `corpusforge-core`: shared errors, result types, seed handling, deterministic stream primitives, and shared domain types.
- `corpusforge-cff`: implemented `.cff` v0 profile pack reader, writer, verifier, versioning, and hashing logic.
- `corpusforge-profile`: deterministic fixture profile compilation into `.cff` v0 profile packs with embedded byte-level n-gram models, with broader profile compilation still planned.
- `corpusforge-unicode`: implemented fixture-based Unicode adversarial valid-text and raw-byte APIs, including mode/output validation for grapheme, bidi, zero-width, emoji, normalization, mixed, and invalid-utf8 cases. This crate does not yet provide broad Unicode mutation or confusable generation.
- `corpusforge-ngram`: implemented byte-level weighted bigram profile building and deterministic sampling.
- `corpusforge-shrink`: implemented byte-level reducer and minimizer logic using external stdin predicates. It is independent from specific parsers or tokenizers, but it is not Unicode-aware or structure-aware.
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

The current CLI is intentionally narrow. Implemented profile commands can build, inspect, and verify `.cff` v0 fixture profile packs. Implemented generation can produce fixture-based Unicode cases and byte-level profile-backed n-gram output. Implemented shrink and replay behavior is limited to the Milestone 7 MVP.

`invalid-utf8` is intentionally limited to raw-byte output; valid-text output rejects it because it may produce bytes that cannot decode as UTF-8.

The shrinker operates on bytes only. It invokes predicate commands directly, writes candidate bytes to stdin, preserves repeatable nonzero exits or consistent timeouts, and rejects flaky predicate behavior. It does not understand Unicode grapheme boundaries, syntax trees, parser states, or structured formats.

Replay currently requires direct flags: a `.cff` profile with an embedded n-gram model, `--seed` or `--seed-file`, and a half-open `--range <start>..<end>`. It does not consume saved metadata files. Without `--out`, replay writes binary bytes to stdout; `--json` requires `--out`.

The v0 architecture is still subject to change. Public command names should be treated as early project direction. The `.cff` v0 format is implemented for current fixtures, but it is unstable and has no cross-version compatibility guarantee. Corpus output behavior is not stable until later milestones define and test CLI integration, report formats, and broader generation workflows.
