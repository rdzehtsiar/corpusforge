// SPDX-License-Identifier: Apache-2.0

//! Deterministic byte-level n-gram generation.

use std::fs;
use std::io::Write;
use std::path::Path;

use corpusforge_core::rng::{DeterministicStream, DOMAIN_NGRAM};
use corpusforge_core::seed::MasterSeed;
use corpusforge_core::weighted::WeightedTable;
use corpusforge_core::{CorpusForgeError, Result};

/// Stable engine identifier for byte bigram models.
pub const ENGINE_NAME: &str = "corpusforge.byte_bigram";
/// Stable byte bigram model format version.
pub const ENGINE_VERSION: u16 = 0;

const MODEL_MAGIC: &[u8; 8] = b"CFBGV0\0\0";
const STREAM_CONTEXT: &[u8] = b"corpusforge-ngram.byte-bigram.v0";
const BYTE_VALUES: usize = 256;
const IO_BUFFER_SIZE: usize = 8192;

/// Deterministic byte-level bigram model compiled from raw byte fixtures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteBigramModel {
    starts: WeightedByteChoices,
    transitions: Vec<ByteTransition>,
}

impl ByteBigramModel {
    /// Compiles a model from raw byte slices.
    pub fn compile_from_slices<B>(fixtures: impl IntoIterator<Item = B>) -> Result<Self>
    where
        B: AsRef<[u8]>,
    {
        compile_model_from_slices(fixtures)
    }

    /// Generates exactly `byte_count` bytes into `writer`.
    pub fn generate_bytes<W: Write>(
        &self,
        seed: &MasterSeed,
        byte_count: usize,
        writer: W,
    ) -> Result<()> {
        generate_bytes(self, seed, byte_count, writer)
    }

    /// Serializes this model using a stable little-endian byte encoding.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MODEL_MAGIC);
        bytes.extend_from_slice(&ENGINE_VERSION.to_le_bytes());
        write_choices(&mut bytes, &self.starts);
        bytes.extend_from_slice(&(self.transitions.len() as u16).to_le_bytes());

        for transition in &self.transitions {
            bytes.push(transition.previous);
            write_choices(&mut bytes, &transition.next);
        }

        bytes
    }

    /// Deserializes a model produced by [`ByteBigramModel::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ModelReader::new(bytes).read_model()
    }

    /// Returns stable start choices as `(byte, count)` pairs sorted by byte value.
    pub fn start_counts(&self) -> Vec<(u8, u64)> {
        self.starts.counts()
    }

    /// Returns stable transition choices for `previous` as `(byte, count)` pairs sorted by byte value.
    pub fn transition_counts(&self, previous: u8) -> Vec<(u8, u64)> {
        self.transition_for(previous)
            .map_or_else(Vec::new, |choices| choices.counts())
    }

    fn choose_start(&self, stream: &mut DeterministicStream) -> Result<u8> {
        self.starts.choose(stream)
    }

    fn choose_next(&self, previous: u8, stream: &mut DeterministicStream) -> Result<u8> {
        match self.transition_for(previous) {
            Some(choices) => choices.choose(stream),
            None => self.choose_start(stream),
        }
    }

    fn transition_for(&self, previous: u8) -> Option<&WeightedByteChoices> {
        self.transitions
            .binary_search_by_key(&previous, |transition| transition.previous)
            .ok()
            .map(|index| &self.transitions[index].next)
    }
}

/// Compiles a byte bigram model from raw byte slices.
pub fn compile_model_from_slices<B>(
    fixtures: impl IntoIterator<Item = B>,
) -> Result<ByteBigramModel>
where
    B: AsRef<[u8]>,
{
    let mut start_counts = [0_u64; BYTE_VALUES];
    let mut transition_counts = vec![[0_u64; BYTE_VALUES]; BYTE_VALUES];
    let mut non_empty_fixture_count = 0_u64;

    for fixture in fixtures {
        let fixture = fixture.as_ref();
        if fixture.is_empty() {
            continue;
        }

        non_empty_fixture_count = non_empty_fixture_count.checked_add(1).ok_or_else(|| {
            CorpusForgeError::invalid_argument(
                "byte bigram fixture count overflowed u64 during compilation",
            )
        })?;
        increment_count(&mut start_counts[fixture[0] as usize], "start byte")?;

        for pair in fixture.windows(2) {
            let previous = pair[0] as usize;
            let next = pair[1] as usize;
            increment_count(
                &mut transition_counts[previous][next],
                "byte bigram transition",
            )?;
        }
    }

    if non_empty_fixture_count == 0 {
        return Err(CorpusForgeError::invalid_argument(
            "byte bigram model requires at least one non-empty fixture",
        ));
    }

    let starts = WeightedByteChoices::from_counts(start_counts)?;
    let mut transitions = Vec::new();
    for (previous, counts) in transition_counts.into_iter().enumerate() {
        if let Some(next) = WeightedByteChoices::from_counts_if_non_zero(counts) {
            transitions.push(ByteTransition {
                previous: previous as u8,
                next: next?,
            });
        }
    }

    Ok(ByteBigramModel {
        starts,
        transitions,
    })
}

/// Reads each file as raw bytes and compiles a byte bigram model.
pub fn compile_model_from_files<P>(paths: impl IntoIterator<Item = P>) -> Result<ByteBigramModel>
where
    P: AsRef<Path>,
{
    let fixtures = paths
        .into_iter()
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()?;
    let slices = fixtures.iter().map(Vec::as_slice);

    compile_model_from_slices(slices)
}

/// Generates exactly `byte_count` bytes from `model` into `writer`.
pub fn generate_bytes<W: Write>(
    model: &ByteBigramModel,
    seed: &MasterSeed,
    byte_count: usize,
    mut writer: W,
) -> Result<()> {
    if byte_count == 0 {
        return Ok(());
    }

    let mut stream =
        DeterministicStream::from_seed_with_context(seed, DOMAIN_NGRAM, STREAM_CONTEXT);
    let mut buffer = Vec::with_capacity(IO_BUFFER_SIZE.min(byte_count));
    let mut previous = model.choose_start(&mut stream)?;

    push_generated_byte(&mut buffer, &mut writer, previous)?;

    for _ in 1..byte_count {
        let next = model.choose_next(previous, &mut stream)?;
        push_generated_byte(&mut buffer, &mut writer, next)?;
        previous = next;
    }

    if !buffer.is_empty() {
        writer.write_all(&buffer)?;
    }

    Ok(())
}

fn push_generated_byte<W: Write>(buffer: &mut Vec<u8>, writer: &mut W, byte: u8) -> Result<()> {
    buffer.push(byte);
    if buffer.len() == IO_BUFFER_SIZE {
        writer.write_all(buffer)?;
        buffer.clear();
    }

    Ok(())
}

fn increment_count(count: &mut u64, label: &'static str) -> Result<()> {
    *count = count.checked_add(1).ok_or_else(|| {
        CorpusForgeError::invalid_argument(format!("{label} count overflowed u64"))
    })?;

    Ok(())
}

fn write_choices(bytes: &mut Vec<u8>, choices: &WeightedByteChoices) {
    bytes.extend_from_slice(&(choices.bytes.len() as u16).to_le_bytes());

    for (&byte, &weight) in choices.bytes.iter().zip(&choices.weights) {
        bytes.push(byte);
        bytes.extend_from_slice(&weight.to_le_bytes());
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ByteTransition {
    previous: u8,
    next: WeightedByteChoices,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WeightedByteChoices {
    bytes: Vec<u8>,
    weights: Vec<u64>,
    table: WeightedTable,
}

impl WeightedByteChoices {
    fn from_counts(counts: [u64; BYTE_VALUES]) -> Result<Self> {
        Self::from_counts_if_non_zero(counts).ok_or_else(|| {
            CorpusForgeError::invalid_argument("byte choice table requires a non-zero total count")
        })?
    }

    fn from_counts_if_non_zero(counts: [u64; BYTE_VALUES]) -> Option<Result<Self>> {
        let mut bytes = Vec::new();
        let mut weights = Vec::new();

        for (byte, count) in counts.into_iter().enumerate() {
            if count > 0 {
                bytes.push(byte as u8);
                weights.push(count);
            }
        }

        if bytes.is_empty() {
            None
        } else {
            Some(Self::new(bytes, weights))
        }
    }

    fn new(bytes: Vec<u8>, weights: Vec<u64>) -> Result<Self> {
        validate_choices(&bytes, &weights)?;
        let table = WeightedTable::new(weights.iter().copied())?;

        Ok(Self {
            bytes,
            weights,
            table,
        })
    }

    fn choose(&self, stream: &mut DeterministicStream) -> Result<u8> {
        let index = self.table.choose_index(stream)?;

        Ok(self.bytes[index])
    }

    fn counts(&self) -> Vec<(u8, u64)> {
        self.bytes
            .iter()
            .copied()
            .zip(self.weights.iter().copied())
            .collect()
    }
}

fn validate_choices(bytes: &[u8], weights: &[u64]) -> Result<()> {
    if bytes.len() != weights.len() {
        return Err(CorpusForgeError::invalid_argument(
            "byte choice table byte and weight lengths differ",
        ));
    }

    if bytes.is_empty() {
        return Err(CorpusForgeError::invalid_argument(
            "byte choice table requires at least one byte",
        ));
    }

    let mut previous = None;
    for (&byte, &weight) in bytes.iter().zip(weights) {
        if weight == 0 {
            return Err(CorpusForgeError::invalid_argument(
                "byte choice table weights must be non-zero",
            ));
        }

        if previous.is_some_and(|previous| previous >= byte) {
            return Err(CorpusForgeError::invalid_argument(
                "byte choice table bytes must be strictly sorted by byte value",
            ));
        }
        previous = Some(byte);
    }

    Ok(())
}

struct ModelReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ModelReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_model(&mut self) -> Result<ByteBigramModel> {
        self.expect_magic()?;
        let version = self.read_u16("model version")?;
        if version != ENGINE_VERSION {
            return Err(CorpusForgeError::unsupported_version(format!(
                "byte bigram model version {version} is unsupported; expected {ENGINE_VERSION}"
            )));
        }

        let starts = self.read_choices("start table")?;
        let transition_count = self.read_u16("transition count")?;
        let mut transitions = Vec::with_capacity(transition_count as usize);
        let mut previous_transition = None;

        for _ in 0..transition_count {
            let previous = self.read_u8("transition previous byte")?;
            if previous_transition.is_some_and(|seen| seen >= previous) {
                return Err(CorpusForgeError::invalid_argument(
                    "byte bigram model transitions must be strictly sorted by previous byte",
                ));
            }

            let next = self.read_choices("transition table")?;
            transitions.push(ByteTransition { previous, next });
            previous_transition = Some(previous);
        }

        if self.offset != self.bytes.len() {
            return Err(CorpusForgeError::invalid_argument(
                "byte bigram model contains trailing bytes after the transition table",
            ));
        }

        Ok(ByteBigramModel {
            starts,
            transitions,
        })
    }

    fn expect_magic(&mut self) -> Result<()> {
        let magic = self.read_exact(MODEL_MAGIC.len(), "model magic")?;
        if magic != MODEL_MAGIC {
            return Err(CorpusForgeError::invalid_argument(
                "byte bigram model has an invalid magic header",
            ));
        }

        Ok(())
    }

    fn read_choices(&mut self, label: &'static str) -> Result<WeightedByteChoices> {
        let count = self.read_u16(label)? as usize;
        let mut bytes = Vec::with_capacity(count);
        let mut weights = Vec::with_capacity(count);
        let mut previous = None;

        for _ in 0..count {
            let byte = self.read_u8(label)?;
            if previous.is_some_and(|seen| seen >= byte) {
                return Err(CorpusForgeError::invalid_argument(format!(
                    "byte bigram model {label} bytes must be strictly sorted by byte value"
                )));
            }

            bytes.push(byte);
            weights.push(self.read_u64(label)?);
            previous = Some(byte);
        }

        WeightedByteChoices::new(bytes, weights)
    }

    fn read_u8(&mut self, label: &'static str) -> Result<u8> {
        Ok(self.read_exact(1, label)?[0])
    }

    fn read_u16(&mut self, label: &'static str) -> Result<u16> {
        let bytes = self.read_exact(2, label)?;

        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u64(&mut self, label: &'static str) -> Result<u64> {
        let bytes = self.read_exact(8, label)?;

        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_exact(&mut self, len: usize, label: &'static str) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(len).ok_or_else(|| {
            CorpusForgeError::invalid_argument("byte bigram model offset overflowed usize")
        })?;
        let bytes = self.bytes.get(self.offset..end).ok_or_else(|| {
            CorpusForgeError::invalid_argument(format!(
                "byte bigram model ended while reading {label}"
            ))
        })?;
        self.offset = end;

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compile_model_from_slices, generate_bytes, ByteBigramModel, ENGINE_NAME, ENGINE_VERSION,
    };
    use corpusforge_core::seed::MasterSeed;
    use corpusforge_core::CorpusForgeError;
    use std::str::FromStr;

    #[test]
    fn exposes_engine_metadata() {
        assert_eq!(ENGINE_NAME, "corpusforge.byte_bigram");
        assert_eq!(ENGINE_VERSION, 0);
    }

    #[test]
    fn compilation_rejects_no_non_empty_bytes() {
        let error = compile_model_from_slices([b"".as_slice(), &[]])
            .expect_err("all-empty fixtures should be rejected");

        assert_invalid_argument_contains(error, "at least one non-empty fixture");
    }

    #[test]
    fn single_byte_fixture_generates_exact_repeated_bytes() {
        let model = compile_model_from_slices([b"Z".as_slice()]).expect("fixture is non-empty");
        let mut output = Vec::new();

        generate_bytes(&model, &seed_1337(), 9, &mut output).expect("generation should succeed");

        assert_eq!(output, b"ZZZZZZZZZ");
    }

    #[test]
    fn generated_length_matches_requested_count_including_zero() {
        let model = compile_model_from_slices([b"ababa".as_slice()]).expect("fixture is non-empty");
        let mut empty = Vec::new();
        let mut bytes = Vec::new();

        generate_bytes(&model, &seed_1337(), 0, &mut empty)
            .expect("zero generation should succeed");
        generate_bytes(&model, &seed_1337(), 257, &mut bytes).expect("generation should succeed");

        assert!(empty.is_empty());
        assert_eq!(bytes.len(), 257);
    }

    #[test]
    fn same_seed_and_model_generate_identical_bytes() {
        let model = compile_model_from_slices([b"aba".as_slice(), b"abb".as_slice(), b"bcc"])
            .expect("fixtures are non-empty");
        let mut left = Vec::new();
        let mut right = Vec::new();

        generate_bytes(&model, &seed_1337(), 64, &mut left).expect("generation should succeed");
        generate_bytes(&model, &seed_1337(), 64, &mut right).expect("generation should succeed");

        assert_eq!(left, right);
    }

    #[test]
    fn different_seeds_generate_different_bytes_where_practical() {
        let model = compile_model_from_slices([b"abacadaba".as_slice(), b"acadaeaf".as_slice()])
            .expect("fixtures are non-empty");
        let mut left = Vec::new();
        let mut right = Vec::new();

        generate_bytes(&model, &seed_1337(), 64, &mut left).expect("generation should succeed");
        generate_bytes(&model, &seed_42(), 64, &mut right).expect("generation should succeed");

        assert_ne!(left, right);
    }

    #[test]
    fn serialization_round_trips_and_preserves_generation() {
        let model = compile_model_from_slices([
            b"hello".as_slice(),
            b"hostile".as_slice(),
            b"\xFF\x00\xFF".as_slice(),
        ])
        .expect("fixtures are non-empty");
        let decoded =
            ByteBigramModel::from_bytes(&model.to_bytes()).expect("serialized model should decode");
        let mut original = Vec::new();
        let mut round_tripped = Vec::new();

        generate_bytes(&model, &seed_1337(), 80, &mut original).expect("generation should succeed");
        generate_bytes(&decoded, &seed_1337(), 80, &mut round_tripped)
            .expect("generation should succeed");

        assert_eq!(model, decoded);
        assert_eq!(original, round_tripped);
    }

    #[test]
    fn stable_counts_are_sorted_by_byte_value() {
        let model = compile_model_from_slices([b"ba".as_slice(), b"ca".as_slice(), b"cb"])
            .expect("fixtures are non-empty");

        assert_eq!(model.start_counts(), vec![(b'b', 1), (b'c', 2)]);
        assert_eq!(model.transition_counts(b'c'), vec![(b'a', 1), (b'b', 1)]);
        assert!(model.transition_counts(b'a').is_empty());
    }

    #[test]
    fn stable_golden_like_generation_for_small_fixture_and_seed() {
        let model = compile_model_from_slices([b"ababa".as_slice(), b"acaca".as_slice(), b"adada"])
            .expect("fixtures are non-empty");
        let mut output = Vec::new();

        generate_bytes(&model, &seed_1337(), 24, &mut output).expect("generation should succeed");

        assert_eq!(output, b"adadacadacadababadacacad");
    }

    #[test]
    fn invalid_serialized_model_is_rejected_cleanly() {
        let error = ByteBigramModel::from_bytes(b"not a model")
            .expect_err("invalid model bytes should be rejected");

        assert_invalid_argument_contains(error, "invalid magic header");
    }

    fn seed_1337() -> MasterSeed {
        MasterSeed::from_str("1337").expect("integer seed 1337 should parse")
    }

    fn seed_42() -> MasterSeed {
        MasterSeed::from_str("42").expect("integer seed 42 should parse")
    }

    fn assert_invalid_argument_contains(error: CorpusForgeError, expected: &str) {
        assert_eq!(error.category(), "invalid_argument");
        assert!(
            error.to_string().contains(expected),
            "expected '{error}' to contain '{expected}'"
        );
    }
}
