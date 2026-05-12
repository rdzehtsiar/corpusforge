# Examples

This directory contains small offline demos for CorpusForge workflows.

- `reject_invalid_utf8.rs`: a dependency-free stdin harness that exits with a
  nonzero status when input bytes are not valid UTF-8. It is used by the
  tokenizer workflow demo in [docs/tokenizer-workflow.md](../docs/tokenizer-workflow.md).

Examples should stay small, offline, deterministic, and clear about which
CorpusForge behavior they exercise.
