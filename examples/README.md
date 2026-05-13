# Examples

This directory contains small offline demos for CorpusForge workflows.

- `reject_invalid_utf8.rs`: a dependency-free stdin harness that exits with a
  nonzero status when input bytes are not valid UTF-8. It is used by the
  tokenizer workflow demo in [docs/tokenizer-workflow.md](../docs/tokenizer-workflow.md).
- `reject_open_markdown_fence.rs`: a dependency-free stdin harness that exits
  with a nonzero status for intentionally narrow Markdown renderer-style checks,
  including unclosed fenced code blocks and open inline links/images. It is used
  by the grammar workflow demo in [docs/grammar-workflow.md](../docs/grammar-workflow.md).
- `reject_malformed_json.rs`: a dependency-free stdin harness that exits with a
  nonzero status for simple JSON structural problems, such as unbalanced
  delimiters or trailing commas. It is used by the grammar workflow demo in
  [docs/grammar-workflow.md](../docs/grammar-workflow.md).

Examples should stay small, offline, deterministic, and clear about which
CorpusForge behavior they exercise.
