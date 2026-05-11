# Determinism

## Contract Draft

CorpusForge is designed around reproducible hostile-text workflows. The intended contract is:

```text
same tool version
+ same profile content and profile hash
+ same seed
+ same command flags
+ same determinism mode
= same observable output bytes and reports
```

This is a draft contract. Milestone 1 does not implement corpus generation, `.cff` profiles, deterministic streams, replay, or shrinking. The contract becomes enforceable only as those systems are implemented and covered by tests.

## v0 Stability Warning

All v0 behavior is unstable until explicitly documented otherwise. During early milestones, the project may change:

- seed syntax
- deterministic stream derivation
- profile format layout
- profile hash rules
- command flags
- report formats
- generated output semantics

Any future compatibility guarantee must identify the exact version, format, command, and tests that support it.

## Required Invariants

Future deterministic behavior must preserve these invariants:

- Seeds are parsed explicitly and invalid seeds fail with clear diagnostics.
- Independent generation components use domain-separated streams.
- Profile content and profile version participate in reproducibility metadata.
- User-visible file traversal, diagnostics, reports, and compatibility results are sorted by stable keys.
- Generation paths that must be reproducible use integer probability tables or equivalent deterministic discrete sampling.
- Default reports and golden outputs avoid timestamps, absolute paths, locale-dependent formatting, and host-specific state.
- Valid text modes and raw byte modes remain separate, especially for invalid UTF-8 cases.

## Offline and Privacy Defaults

The default CorpusForge workflow must be offline. The default binary must not make network calls, require a cloud account, upload crashes, check for updates, or collect telemetry.

Adding network, telemetry, analytics, or runtime ML dependencies requires explicit approval and must not affect the default offline binary.

## Unsupported Behavior

Milestone 1 does not support:

- deterministic corpus output
- replay from seed/profile/range metadata
- shrinking or predicate execution
- `.cff` verification or compatibility checks
- Unicode mutation modes
- byte-level invalid UTF-8 generation
- machine-readable CI reports

The CLI may expose planned commands for these workflows, but command execution is not implemented yet.
