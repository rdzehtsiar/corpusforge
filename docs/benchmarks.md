# Benchmarks

Benchmarks in this document are local evidence for the listed command and
environment only. They are not compatibility guarantees, release promises, or
claims about other machines.

## Byte Bigram Generation

Observed on 2026-05-12 in the local Codex workspace:

- OS: Microsoft Windows NT 10.0.26200.0
- CPU environment: `Intel64 Family 6 Model 183 Stepping 1, GenuineIntel`,
  `NUMBER_OF_PROCESSORS=24`
- Toolchain: `rustc 1.95.0 (59807616e 2026-04-14)`,
  `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`
- Profile source: repository `tests\fixtures`
- Profile hash:
  `cff:d2fb375e2bda819d4746e0077823653fee6704c314d2c99e40953374add636c6`
- Build command:
  `target\release\corpusforge.exe profile build tests\fixtures --out target\bench-fixtures.cff`
- Measured command:
  `target\release\corpusforge.exe gen --profile target\bench-fixtures.cff --seed 1337 --bytes 1MB --out target\bench-ngram.bin`
- Output size: 1,048,576 bytes
- Elapsed time: 47.5139 ms via PowerShell `Measure-Command`
- Observed throughput: approximately 21.05 MiB/s

The benchmark used release binaries and wrote outputs under `target\`. The
generated `.cff` and `.bin` files are build artifacts and are not committed.
