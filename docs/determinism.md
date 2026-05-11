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
and deterministic fixture profile compilation. Corpus generation, Unicode
mutation, replay, shrinking, and CI reports are still not implemented.

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

These fixtures assert the current core APIs exactly. Additional `.cff` and
profile fixtures cover current v0 serialization, verification, hashing, and
deterministic fixture profile compilation. They do not claim corpus generation,
Unicode mutation, replay, shrink, broad CLI output, report, or cross-version
format compatibility.

## CLI Status

The CLI currently exposes planned command names and parses common flags such as
`--seed`, `--seed-file`, `--profile`, `--out`, `--bytes`, `--determinism`,
`--metadata-out`, `--quiet`, and `--json` for placeholder commands.

Profile build, inspect, and verify command behavior exists for implemented
`.cff` v0 fixture profile workflows. Command execution for generation,
shrinking, replay, and CI reporting is not implemented. CLI generation flags do
not yet connect to deterministic stream construction, weighted sampling, profile
loading for generation, or output generation.

## Offline and Privacy Defaults

The default CorpusForge workflow must be offline. The default binary must not
make network calls, require a cloud account, upload crashes, check for updates,
or collect telemetry.

Adding network, telemetry, analytics, or runtime ML dependencies requires
explicit approval and must not affect the default offline binary.

## Current Limitations

Unsupported behavior includes:

- deterministic corpus output
- stable `.cff` cross-version compatibility guarantees
- Unicode mutation modes
- weighted n-gram corpus generation
- byte-level invalid UTF-8 generation
- replay from seed/profile/range metadata
- shrinking or predicate execution
- machine-readable CI reports
- cross-version deterministic compatibility guarantees

Future compatibility guarantees must identify the exact version, format,
command, and tests that support them.
