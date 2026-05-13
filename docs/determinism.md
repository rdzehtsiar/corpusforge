# Determinism

## Current Scope

CorpusForge is designed around reproducible hostile-text workflows. The intended
contract is:

```text
same tool version
+ same profile content and profile hash
+ same seed
+ same command flags
+ same determinism mode
= same observable output bytes and reports
```

The current implementation covers part of that contract: master seed parsing,
domain-separated deterministic streams, integer bounded sampling,
integer-only weighted choice, `.cff` v0 serialization, verification, hashing,
deterministic fixture profile compilation, and fixture-based Unicode valid-text
and raw-byte generation in `corpusforge-unicode`. It also includes
byte-level n-gram profile generation, stable tokenizer CI reports for the
implemented built-in workflow, built-in fixture/template-based grammar
generation, byte-level shrink with stdin predicates, and profile-backed replay
by byte range. Profile-driven Unicode generation, `.cff` profile-backed
grammar generation, and broader CI reports are still not implemented.

All v0 deterministic behavior is unstable until explicitly versioned with a
compatibility guarantee and supporting tests.

## Implemented Seed Formats

`corpusforge-core` represents a master seed as exactly 32 bytes.

Implemented seed inputs:

- Decimal integer text, such as `1337`.
- Hex seed text with the `hex:` prefix followed by exactly 64 hex characters.
- Seed files containing exactly 32 raw bytes.

Decimal integer seeds must contain only ASCII digits and must not be empty.
Leading zeroes are canonicalized away before hashing, so `42` and `00042`
produce the same master seed. The canonical decimal string `0` is used for
inputs containing only zeroes.

Integer seed expansion is:

```text
BLAKE3("corpusforge.master_seed.integer.v1\0" || canonical_decimal_ascii)
```

The 32-byte BLAKE3 output is the master seed. Display formatting renders master
seeds as lowercase 64-character hex.

Hex seed text is decoded directly into the 32-byte master seed. Uppercase and
lowercase hex digits are accepted, and display formatting remains lowercase.

## Stream Domains

Implemented stream domains are explicit byte labels:

| Constant | Label |
| --- | --- |
| `DOMAIN_ROOT` | `corpusforge/v0/root` |
| `DOMAIN_PROFILE` | `corpusforge/v0/profile` |
| `DOMAIN_NGRAM` | `corpusforge/v0/ngram` |
| `DOMAIN_UNICODE` | `corpusforge/v0/unicode` |
| `DOMAIN_GRAMMAR` | `corpusforge/v0/grammar` |
| `DOMAIN_CORRUPTION` | `corpusforge/v0/corruption` |
| `DOMAIN_SHRINK` | `corpusforge/v0/shrink` |
| `DOMAIN_REPLAY` | `corpusforge/v0/replay` |

Stream seed derivation is:

```text
BLAKE3(master_seed || domain_label || context)
```

`context` is optional byte input. The no-context constructor uses empty context
bytes.

The 32-byte BLAKE3 output seeds `rand_chacha::ChaCha20Rng`. The implemented
stream API exposes `next_u32`, `next_u64`, `fill_bytes`, and `usize_below`.
There is no implemented seek, skip-ahead, named counter, or stream transcript
format yet.

## Integer Sampling

`usize_below(bound)` and weighted choice use integer rejection sampling to avoid
modulo bias. A zero bound returns an `invalid_argument` project error.

For a non-zero `u64` bound, sampling uses:

```text
threshold = bound.wrapping_neg() % bound
repeat:
  value = stream.next_u64()
  if value >= threshold:
    return value % bound
```

`WeightedTable` stores cumulative `u64` weights in input order and rejects empty
tables, zero total weight, and total-weight overflow. `choose_index` samples a
target in `0..total_weight` with the rule above, then returns the first index
whose cumulative weight is greater than the target. Zero-weight entries are
therefore never selected.

## Golden Fixtures

Small golden fixtures under `tests/golden` currently cover:

- seed `1337`, `DOMAIN_NGRAM`, first 32 stream bytes as hex
- seed `1337`, `DOMAIN_UNICODE`, first 32 stream bytes as hex
- seed `1337`, `DOMAIN_NGRAM`, context `weighted`,
  `WeightedTable::new([1, 3, 6, 10])`, first 16 selected indexes
- seed `1337`, `generate_valid_text(..., UnicodeMode::Grapheme, 12)` as hex
- seed `1337`, `generate_valid_text(..., UnicodeMode::Mixed, 12)` as hex
- seed `1337`, `generate_raw_bytes(..., UnicodeMode::InvalidUtf8, 12)` as hex
- seed `1337`, `generate_raw_bytes(..., UnicodeMode::Mixed, 12)` as hex

These fixtures assert the current core and Unicode fixture APIs exactly.
Additional `.cff` and profile fixtures cover current v0 serialization,
verification, hashing, and deterministic fixture profile compilation. They do
not claim broad Unicode compatibility, full Markdown or JSON conformance,
structure-aware shrink behavior, metadata-file-driven replay, broad CLI
output, broad report coverage, or cross-version format compatibility.

## Unicode Output Boundaries

`corpusforge-unicode` currently implements deterministic fixture-based
generation at two explicit output boundaries:

- `generate_valid_text` returns valid UTF-8 text and supports `grapheme`,
  `bidi`, `zero-width`, `emoji`, `normalization`, and `mixed`.
- `generate_raw_bytes` returns bytes and supports `grapheme`, `bidi`,
  `zero-width`, `emoji`, `normalization`, `mixed`, and `invalid-utf8`.

The `invalid-utf8` mode is rejected for valid-text output and is emitted only
through raw-byte generation. Valid-text modes produce UTF-8 bytes when requested
through the raw-byte API. `mixed` valid-text output samples only valid-text
fixture families, while `mixed` raw-byte output can include invalid UTF-8
fixtures.

This is a fixture model, not a complete Unicode mutation engine. It covers
representative adversarial fixture families for the implemented modes, but it
does not establish broad parser, tokenizer, renderer, or Unicode conformance
coverage.

## Grammar Output Boundaries

`corpusforge-grammar` currently implements deterministic fixture/template-based
generation for Markdown and JSON in `valid`, `near-valid`, and `malformed`
modes. Grammar streams use `DOMAIN_GRAMMAR` with a context derived from the
grammar format, grammar mode, optional Unicode composition mode, and current
case shape.

Grammar output is always valid UTF-8 text. The grammar generator can optionally
compose valid-text Unicode fixture modes into leaf content, but it rejects
`invalid-utf8` because raw invalid bytes do not compose with grammar output.
The current grammar implementation is built in and is not backed by `.cff`
profiles yet.

This is not a complete Markdown or JSON conformance suite. It provides
representative deterministic cases for local parser or renderer harnesses, but
it does not establish broad format compatibility or parser correctness claims.

## CLI Status

Profile build, inspect, and verify command behavior exists for implemented
`.cff` v0 fixture profile workflows. `corpusforge gen` supports implemented
fixture-based Unicode modes, byte-level profile-backed n-gram generation, and
built-in grammar generation through `--grammar markdown|json --grammar-mode valid|near-valid|malformed`.
`corpusforge ci tokenizer` supports the implemented built-in tokenizer stdin
harness workflow and stable JSON reports.

`corpusforge shrink` implements byte-level minimization of an input file while
preserving the original predicate failure signature. The predicate executable
is invoked directly without a shell. Each candidate byte sequence is written to
predicate stdin. Exit code `0` means the candidate passed, and a nonzero exit
code is the failure signature to preserve. A timeout is a preservable failure
signature only when the original input consistently times out. Flaky predicates
are rejected when repeated runs disagree. Defaults are `--timeout-ms 1000` and
`--max-runs 10000`.

`corpusforge replay` implements profile-backed replay by byte range. It reads a
`.cff` profile with an embedded n-gram model, accepts either `--seed` or
`--seed-file`, and emits the half-open `--range <start>..<end>`. Without
`--out`, replay writes binary bytes directly to stdout. `--json` requires
`--out` because stdout is otherwise reserved for replayed bytes.

Shrink and replay metadata JSON is stable and does not include timestamps.
Replay is direct-flag driven; it does not consume a saved metadata file yet.
The shrinker operates on bytes and is not Unicode-aware or structure-aware.

## Offline and Privacy Defaults

The default CorpusForge workflow must be offline. The default binary must not
make network calls, require a cloud account, upload crashes, check for updates,
or collect telemetry.

Adding network, telemetry, analytics, or runtime ML dependencies requires
explicit approval and must not affect the default offline binary.

## Current Limitations

Unsupported behavior includes:

- stable `.cff` cross-version compatibility guarantees
- broad Unicode mutation or compatibility guarantees beyond the fixture APIs
- `.cff` profile-backed grammar generation
- full Markdown or JSON conformance-suite behavior
- structure-aware or Unicode-aware shrinking
- replay from saved metadata files
- grammar-specific CI reports
- broad machine-readable CI reports beyond the tokenizer workflow
- cross-version deterministic compatibility guarantees

Future compatibility guarantees must identify the exact version, format,
command, and tests that support them.
