# Golden Fixtures

This directory contains small deterministic fixtures for implemented core
primitives. They are compatibility evidence for the APIs named below, not
generated corpus output.

- `seed_1337_stream_ngram.hex`: 32 bytes from integer seed `1337` with
  `DeterministicStream::from_seed(..., DOMAIN_NGRAM)`, rendered as lowercase
  hex.
- `seed_1337_stream_unicode.hex`: 32 bytes from integer seed `1337` with
  `DeterministicStream::from_seed(..., DOMAIN_UNICODE)`, rendered as lowercase
  hex.
- `seed_1337_weighted_choice_sequence.txt`: 16 indexes from
  `WeightedTable::new([1, 3, 6, 10])` using integer seed `1337`,
  `DOMAIN_NGRAM`, and context bytes `weighted`.

Golden files must remain deterministic, small, human-readable, and documented
with the API, command, or fixture source that produced them. Do not add broad
generation, shrinking, replay, or Unicode compatibility claims without matching
implementation and tests.
