# Roadmap

CorpusForge is being developed in milestones. The roadmap is directional and should not be treated as a compatibility promise.

| Milestone | Name | Primary Outcome |
|---:|---|---|
| 1 | Product Scope, Repo Foundation, Agent Controls | A controlled Rust workspace with architecture docs, governance, and initial CI. |
| 2 | Deterministic Core and CLI Skeleton | Seed handling, deterministic streams, shared errors, and command structure. |
| 3 | `.cff` Profile Format and Compiler MVP | Versioned profile packs that can be built, inspected, verified, and read. |
| 4 | Unicode Adversarial Engine | Deterministic Unicode and invalid-byte mutation layer. |
| 5 | Weighted N-Gram Generator MVP | Primary generation engine integrated with `.cff` profiles and streaming output. |
| 6 | Tokenizer Torture Workflows | Built-in tokenizer stress modes, external tokenizer harness, and reports. |
| 7 | Shrinker and Minimal Reproducer Workflow | Predicate-driven minimization and deterministic replay. |
| 8 | Grammar-Aware Structured Fuzzing | Markdown and JSON grammar profiles with valid, near-valid, and malformed modes. |
| 9 | CI Integration, Benchmarks, and Quality Gates | CI templates, report formats, performance baselines, and regression harnesses. |
| 10 | Packaging, Documentation, and OSS Launch | Static binaries, signed releases, docs, examples, and launch-ready credibility assets. |

## Current Milestone

Milestone 1 is limited to repository foundation, documentation, agent controls, CI, fixtures, and the CLI skeleton. It does not include generation, `.cff` implementation, Unicode mutation, n-gram training, shrinking, packaging, or release automation.

## Compatibility Note

The roadmap names planned capabilities. Those capabilities become supported only when implemented, tested, documented, and covered by explicit compatibility notes.
