// SPDX-License-Identifier: Apache-2.0

//! Domain-separated deterministic random streams.

use std::fmt::{self, Display, Formatter};

use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use crate::seed::MasterSeed;
use crate::{CorpusForgeError, Result};

/// Root domain used for derivation compatibility checks and root-level streams.
pub const DOMAIN_ROOT: StreamDomain = StreamDomain::new("corpusforge/v0/root");
/// Domain for profile compilation and profile-derived decisions.
pub const DOMAIN_PROFILE: StreamDomain = StreamDomain::new("corpusforge/v0/profile");
/// Domain for weighted n-gram generation.
pub const DOMAIN_NGRAM: StreamDomain = StreamDomain::new("corpusforge/v0/ngram");
/// Domain for Unicode adversarial generation.
pub const DOMAIN_UNICODE: StreamDomain = StreamDomain::new("corpusforge/v0/unicode");
/// Domain for byte or text corruption generation.
pub const DOMAIN_CORRUPTION: StreamDomain = StreamDomain::new("corpusforge/v0/corruption");
/// Domain for shrinking and minimization choices.
pub const DOMAIN_SHRINK: StreamDomain = StreamDomain::new("corpusforge/v0/shrink");
/// Domain for replay-specific deterministic choices.
pub const DOMAIN_REPLAY: StreamDomain = StreamDomain::new("corpusforge/v0/replay");

/// Explicit label for a deterministic stream derivation domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamDomain {
    label: &'static str,
}

impl StreamDomain {
    /// Creates a stream domain from a stable static label.
    pub const fn new(label: &'static str) -> Self {
        Self { label }
    }

    /// Returns the domain label bytes used in stream seed derivation.
    pub const fn as_bytes(self) -> &'static [u8] {
        self.label.as_bytes()
    }

    /// Returns the domain label as text for diagnostics.
    pub const fn as_str(self) -> &'static str {
        self.label
    }
}

impl Display for StreamDomain {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label)
    }
}

/// Deterministic random stream derived from a master seed, domain, and optional context.
#[derive(Clone, Debug)]
pub struct DeterministicStream {
    rng: ChaCha20Rng,
}

impl DeterministicStream {
    /// Derives a deterministic stream without additional context.
    pub fn from_seed(master_seed: &MasterSeed, domain: StreamDomain) -> Self {
        Self::from_seed_with_context(master_seed, domain, [])
    }

    /// Derives a deterministic stream with extra context bytes.
    pub fn from_seed_with_context(
        master_seed: &MasterSeed,
        domain: StreamDomain,
        context: impl AsRef<[u8]>,
    ) -> Self {
        let chacha_seed = derive_chacha_seed(master_seed, domain, context.as_ref());

        Self {
            rng: ChaCha20Rng::from_seed(chacha_seed),
        }
    }

    /// Returns the next deterministic `u32`.
    pub fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    /// Returns the next deterministic `u64`.
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    /// Fills the destination buffer with deterministic bytes.
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng.fill_bytes(dest);
    }

    /// Returns a deterministic value in `0..bound` without modulo bias.
    pub fn usize_below(&mut self, bound: usize) -> Result<usize> {
        if bound == 0 {
            return Err(CorpusForgeError::invalid_argument(
                "bounded deterministic stream sampling requires a non-zero bound",
            ));
        }

        let bound = bound as u64;
        let threshold = bound.wrapping_neg() % bound;

        loop {
            let value = self.next_u64();
            if value >= threshold {
                return Ok((value % bound) as usize);
            }
        }
    }
}

fn derive_chacha_seed(master_seed: &MasterSeed, domain: StreamDomain, context: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(master_seed.as_bytes());
    hasher.update(domain.as_bytes());
    hasher.update(context);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::{
        DeterministicStream, StreamDomain, DOMAIN_NGRAM, DOMAIN_PROFILE, DOMAIN_ROOT,
        DOMAIN_UNICODE,
    };
    use crate::seed::MasterSeed;
    use corpusforge_testkit::TEST_SEED_BYTES;

    const TEST_SEED: MasterSeed = MasterSeed::from_bytes(TEST_SEED_BYTES);

    #[test]
    fn repeated_streams_are_equal() {
        let mut left = DeterministicStream::from_seed(&TEST_SEED, DOMAIN_ROOT);
        let mut right = DeterministicStream::from_seed(&TEST_SEED, DOMAIN_ROOT);

        assert_eq!(left.next_u32(), right.next_u32());
        assert_eq!(left.next_u64(), right.next_u64());

        let mut left_bytes = [0_u8; 64];
        let mut right_bytes = [0_u8; 64];
        left.fill_bytes(&mut left_bytes);
        right.fill_bytes(&mut right_bytes);

        assert_eq!(left_bytes, right_bytes);
    }

    #[test]
    fn different_domains_produce_different_streams() {
        let mut profile = DeterministicStream::from_seed(&TEST_SEED, DOMAIN_PROFILE);
        let mut ngram = DeterministicStream::from_seed(&TEST_SEED, DOMAIN_NGRAM);

        assert_ne!(profile.next_u64(), ngram.next_u64());
    }

    #[test]
    fn different_contexts_produce_different_streams() {
        let mut left =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_UNICODE, b"case-a");
        let mut right =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_UNICODE, b"case-b");

        assert_ne!(left.next_u64(), right.next_u64());
    }

    #[test]
    fn zero_bound_returns_project_error() {
        let mut stream = DeterministicStream::from_seed(&TEST_SEED, DOMAIN_ROOT);

        let error = stream
            .usize_below(0)
            .expect_err("zero bound should return a project error");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("non-zero bound"));
    }

    #[test]
    fn bounded_sampling_stays_in_range_and_is_deterministic() {
        let mut left =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_NGRAM, b"bounds");
        let mut right =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_NGRAM, b"bounds");

        let mut left_values = Vec::new();
        let mut right_values = Vec::new();

        for _ in 0..256 {
            let left_value = left.usize_below(17).expect("bound is non-zero");
            let right_value = right.usize_below(17).expect("bound is non-zero");

            assert!(left_value < 17);
            assert!(right_value < 17);
            left_values.push(left_value);
            right_values.push(right_value);
        }

        assert_eq!(left_values, right_values);
    }

    #[test]
    fn custom_domains_are_explicit_labels() {
        let domain = StreamDomain::new("corpusforge/v0/test");

        assert_eq!(domain.as_str(), "corpusforge/v0/test");
        assert_eq!(domain.as_bytes(), b"corpusforge/v0/test");
    }
}
