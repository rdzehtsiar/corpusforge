# Agent Workflow

CorpusForge expects small, bounded coding-agent tasks. The project is sensitive to determinism, CLI semantics, file formats, and claim language, so broad unreviewed changes are not acceptable.

## Allowed Task Shape

Each task should define:

- one concrete objective
- exact files or crates in scope
- files or areas that must not be touched
- required behavior and explicit non-goals
- tests or checks to run
- acceptance criteria
- known limitations to report

Agents must inspect the repository before making structural changes and must not revert edits made by others.

## Worker and Reviewer Expectations

Use a worker/reviewer loop for implementation tasks:

- The worker implements only the assigned scope.
- The worker reports files changed, design tradeoffs, tests run, and limitations.
- The reviewer checks practical blockers only: correctness, scope creep, missing tests, determinism risks, offline/privacy violations, lint/build risks, and unsupported documentation claims.
- Review should avoid open-ended polishing. If acceptance criteria pass and no practical blocker remains, the task should move forward.
- If review fails, the next worker pass should address the blocker directly and stay within the original scope.

## Required Change Summary

Every PR or handoff summary should include:

- what changed
- why it changed
- how determinism and offline behavior are preserved
- tests or checks run
- known limitations
- follow-up work not included

For documentation-only changes, state that no runtime determinism behavior changed.

## Architecture Review Gates

Require explicit architecture review before changing:

- seed parsing or derivation
- deterministic stream algorithms
- `.cff` format layout, versioning, hashing, or compatibility policy
- generated output semantics
- Unicode category definitions or byte-mode boundaries
- shrinker predicate semantics
- CLI command names, flag names, or exit-code behavior
- report formats or compatibility claims
- release artifacts or packaging guarantees

## Required Gates

Rust changes should pass:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CLI changes should also spot-check:

```powershell
cargo run -p corpusforge-cli -- --help
cargo run -p corpusforge-cli -- profile --help
cargo run -p corpusforge-cli -- gen --help
cargo run -p corpusforge-cli -- shrink --help
```

Documentation changes should be reviewed for conservative claim language and must not describe planned behavior as implemented.
