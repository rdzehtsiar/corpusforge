# AGENTS

This file gives coding agents working in this repository the project context and operating constraints needed to make useful changes.

## Project Context

CorpusForge is planned as an offline, deterministic corpus compiler for adversarial text generation. Its purpose is to help engineers generate hostile, reproducible corpora that expose bugs in parsers, tokenizers, renderers, compression pipelines, Unicode handling, and AI preprocessing systems.

The first product focus is deliberately narrow:

- Unicode-aware tokenizer and parser torture testing
- deterministic generation from seedable profiles
- reproducible failing samples
- shrinking/minimization of text failure cases

The repository is currently in the planning and specification stage. There is no implemented application yet. Do not assume an existing architecture, crate layout, package manager, frontend framework, or test harness unless it is present in the repository.

## Product Boundaries

Keep the project focused on deterministic hostile-text workflows for local engineering use.

In scope:

- deterministic adversarial text and byte corpus generation
- Unicode stress cases, including normalization, bidi, zero-width characters, emoji sequences, confusables, graphemes, and invalid UTF-8 byte cases
- tokenizer, parser, renderer, compression, and preprocessing stress workflows
- seedable profiles and reproducible profile packs
- shrinking/minimization of failing samples
- replay of seed/profile/range-derived cases
- CI-friendly command behavior and machine-readable reports
- transparent file formats and compatibility guarantees
- single static binary distribution goals

Out of initial scope:

- generic AI writing or content generation
- local language model or transformer runtime behavior
- cloud-hosted services
- telemetry, analytics, crash upload, or update checks
- broad support for every file format in the first release
- production data storage or hosted corpus management
- nondeterministic sampling that cannot be reproduced across supported platforms

## Implementation Guidance

When implementation begins, prefer choices that preserve the product promise:

- offline by default
- no telemetry by default
- deterministic state, output, and tests
- reproducible profiles, traces, generated corpora, and minimized cases
- evidence-based compatibility and reliability claims
- clear explanations for malformed input, unsupported modes, and failing predicates
- minimal setup for local users and CI systems

The plan currently recommends a Rust workspace with a CLI-first architecture and eventual single static binary. Treat that as direction from the project plan, but still verify the current repository state before adding tooling or structure.

## Recommended Architecture Direction

Do not create this structure blindly. Use it as guidance once the repository is ready for implementation and the task explicitly calls for project scaffolding.

```text
corpusforge/
  Cargo.toml
  crates/
    corpusforge-cli/          # CLI entrypoint
    corpusforge-core/         # seeds, deterministic streams, errors, shared types
    corpusforge-cff/          # .cff format reader/writer/verifier
    corpusforge-profile/      # profile compiler
    corpusforge-unicode/      # Unicode adversarial layer
    corpusforge-ngram/        # weighted n-gram engine
    corpusforge-grammar/      # grammar engine for later milestones
    corpusforge-shrink/       # reducer/minimizer
    corpusforge-ci/           # report formats and CI helpers
    corpusforge-testkit/      # shared test utilities and golden fixtures
  profiles/
  examples/
  tests/
  docs/
```

Keep concerns separated:

- deterministic seed derivation and stream generation belong in core logic
- `.cff` profile serialization, verification, and hashing belong in a profile-format crate
- Unicode mutation and raw byte generation should remain distinct from grammar-aware generation
- shrinking should be independent from specific parser or tokenizer integrations
- CLI code should orchestrate behavior, not contain core generation logic
- report formats should be stable, sorted, and testable

## Code Quality Requirements

All code changes should be well structured, readable, maintainable, and aligned with clean code and clean architecture practices.

Coding agents must follow these rules:

- Keep module boundaries clear and preserve the intended responsibilities of each crate, package, or module.
- Prefer small, explicit functions with clear names over large procedural blocks.
- Keep seed handling, deterministic streams, profile format logic, Unicode generation, n-gram generation, grammar generation, shrinking, replay, CI reporting, and CLI concerns separated.
- Avoid hidden side effects, global mutable state, and behavior that makes output nondeterministic.
- Prefer deterministic data structures and stable ordering wherever output, reports, profile hashes, fixtures, or diagnostics can be observed.
- Write code that is easy to test, with pure logic separated from filesystem, network, clock, random, and terminal concerns when practical.
- Do not introduce abstractions unless they reduce real duplication, clarify ownership, or match the existing architecture.
- Keep errors explainable and actionable instead of panicking on malformed profiles, invalid input, bad configuration, corrupted state, unsupported modes, failed predicates, or invalid Unicode/byte sequences.
- Follow Rust best practices for ownership, error handling, typed data, concurrency, and dependency use when working in Rust.
- Keep public APIs conservative and documented enough for future crates or integration tests to use safely.

## Test-First Development Requirements

Use a test-first pattern whenever practical.

Testing expectations:

- Write or update tests before implementing behavior changes when the desired behavior can be specified up front.
- Cover every code change with meaningful tests unless there is a documented reason that testing is impractical.
- Improve test coverage while keeping tests practical, maintainable, and tied to real regression risk.
- Do not add shallow tests only to raise a coverage number; tests should prove behavior, edge cases, error handling, and deterministic output.
- Prefer focused unit tests for seed parsing, domain-separated streams, profile hashing, `.cff` encoding/decoding, Unicode mutation, n-gram sampling, byte target behavior, shrinking, replay, report generation, and configuration behavior.
- Prefer fixture and snapshot-style tests for generated byte output, profile inspection output, minimized reproducer metadata, CI reports, and compatibility evidence.
- Include malformed input and negative-path tests where profile parsing, Unicode handling, shrinking predicates, replay metadata, or report generation could otherwise panic or silently misreport.
- Keep tests deterministic, offline, and independent of network access, host-specific absolute paths, wall-clock timestamps, locale-specific behavior, and local machine state.
- When changing existing behavior, update or add regression tests that would fail without the fix.
- If a change cannot reasonably be tested in the current task, state the gap clearly in the final response.

## Determinism Requirements

CorpusForge should produce deterministic behavior wherever users can inspect or compare output:

- Same tool version, profile hash, seed, command flags, and determinism mode should produce identical bytes.
- Use integer probability tables or other deterministic discrete sampling. Do not use floating-point probability logic for generation paths that must be reproducible.
- Use explicit seed parsing and domain-separated substreams for independently reproducible components.
- Sort file traversal results, diagnostics, compatibility results, report output, and user-visible lists by stable keys unless a documented format requires another order.
- Avoid timestamps in default snapshots, golden outputs, or reports unless time is the behavior under test.
- Avoid absolute paths in portable output unless explicitly requested.
- Keep JSON key ordering stable where practical.
- Separate valid text modes from raw byte modes, especially for invalid UTF-8 and byte-level torture cases.
- Snapshot-test core generation paths, profile inspection output, replay output, minimized reproducer metadata, and report formats where practical.

## Non-Negotiable Product Constraints

Preserve these properties unless the user explicitly changes direction:

- Fully offline by default.
- No telemetry by default.
- No hosted backend required.
- No cloud account required for the primary workflow.
- Deterministic generated corpora, profiles, shrink results, replay outputs, and tests where practical.
- Explainable tokenizer, parser, Unicode, byte-level, replay, and shrinking diagnostics.
- CI-friendly behavior.
- Cross-platform support.
- Evidence-based compatibility and reliability claims.
- Clear documentation of unsupported, partial, unstable, or intentionally omitted behavior.

Every diagnostic, report finding, or compatibility claim should make clear:

```text
what happened
where it happened
why it matters
how to fix or reproduce it
what is unsupported, partial, or unstable, if applicable
```

## Strict Source License Header Rule

All coding agents must include SPDX license headers in source code files they create or edit.

This is a strict must-follow rule:

- When creating a source code file, add an SPDX license header before any code.
- When editing an existing source code file, make sure the file already has an SPDX license header; if it does not, add one as part of the edit.
- Use the file's native comment syntax.
- Use the project license identifier: `SPDX-License-Identifier: Apache-2.0`.
- Do not add duplicate SPDX headers when one already exists.
- Do not add SPDX headers to generated files, vendored third-party files, lockfiles, binary files, or data fixtures unless the project later documents a specific convention for those files.

Examples:

```rust
// SPDX-License-Identifier: Apache-2.0
```

```ts
// SPDX-License-Identifier: Apache-2.0
```

```bash
# SPDX-License-Identifier: Apache-2.0
```

## Documentation Guidance

Human-facing documentation should describe what CorpusForge does, who it is for, and the current project state. Avoid exposing unnecessary internal implementation details or private planning details in user-facing docs.

Agent- and contributor-facing documentation may include implementation direction, constraints, and technical priorities, but should remain scoped to engineering work.

Keep docs direct, conservative, and appropriate for an infrastructure debugging tool. Avoid vague claims such as "fully deterministic across every environment", "production-ready", "complete parser coverage", or "AI-powered" unless the repository contains evidence for those claims.

Useful documentation topics as the project matures include:

- architecture
- determinism guarantees
- limitations
- `.cff` profile format
- Unicode behavior and byte-mode behavior
- generation modes
- shrinker and replay behavior
- tokenizer/parser harnesses
- CI report formats
- configuration reference
- contribution guide
- release and binary verification process

## Positioning and Claim Language

Describe CorpusForge precisely:

- deterministic corpus compiler
- parser stress toolkit
- tokenizer torture framework
- Unicode adversarial corpus engine
- reproducible hostile-text generator
- shrinker for text failure cases

Avoid describing CorpusForge as:

- a local LLM
- an AI writer
- a lorem ipsum replacement
- a generic synthetic content engine
- a production data platform

Do not claim broad parser, tokenizer, Markdown, JSON, Unicode, or platform compatibility without tests and documentation naming the verified behavior.

## Working Rules

- Inspect the existing repository before making structural changes.
- Keep changes scoped to the current request.
- Do not introduce frameworks, package managers, generated projects, or large dependencies without a clear need.
- Preserve the offline-first posture.
- Prefer deterministic tests over environment-dependent behavior.
- Document limitations as first-class project information.
- Do not expose private roadmap or business-planning details from planning documents in public-facing documentation.
- Do not revert user changes. If git reports dubious ownership, do not change global git config unless the user asks or the task requires git operations.
- Use imperative mood for commit messages. Prefer subjects such as `Add license metadata`, `Initialize Rust workspace`, or `Document determinism guarantees` instead of past-tense forms such as `Added`, `Initialized`, or `Documented`.
