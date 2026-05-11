# Contributing

CorpusForge is an offline deterministic corpus compiler for hostile text testing. Contributions should preserve that purpose and avoid expanding the project into a generic content generator, hosted service, telemetry product, or runtime ML system.

## Required Checks

Run these checks before proposing changes:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p corpusforge-cli -- --help
```

Run narrower checks while developing when appropriate, but the full set above is the baseline for changes that affect Rust code, command behavior, or public documentation.

## Dependency Policy

- Prefer small, maintained crates with clear ownership, stable releases, and limited transitive dependencies.
- Do not add network libraries without explicit approval. The default tool must remain offline.
- Do not add telemetry, analytics, crash upload, update check, or remote monitoring crates.
- Do not add runtime machine learning or transformer dependencies to the default binary.
- Do not add large frameworks or generated project structures unless a milestone explicitly requires them.
- Keep dependency behavior compatible with deterministic, CI-friendly local execution.

Any proposed dependency should explain why the standard library or an existing crate is insufficient, how the dependency affects determinism, and whether it introduces I/O, networking, background work, platform-specific behavior, or supply-chain risk.

## Unsafe Code

Unsafe Rust is forbidden by workspace lint policy unless the project explicitly changes that policy. Any exception requires:

- a design note explaining why safe Rust is insufficient
- a narrow safety invariant
- focused tests or verification notes
- expert approval before implementation

## Protected Decisions

Do not change these areas casually:

- seed parsing and seed derivation
- deterministic stream algorithms
- `.cff` profile layout, versioning, hashing, and compatibility rules
- generated output semantics
- CLI command names and exit-code behavior
- Unicode category definitions and byte-mode boundaries
- report formats and compatibility claims

Changes in these areas need architecture review before implementation expands.

## Documentation Standards

Documentation should be direct and evidence-based. Do not claim generation, format compatibility, parser coverage, Unicode correctness, static packaging, or production readiness before the repository contains implemented behavior and tests that support the claim.

Clearly label unsupported, partial, unstable, and planned behavior.
