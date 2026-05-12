// SPDX-License-Identifier: Apache-2.0

//! Integer-only weighted choice utilities.

use crate::rng::DeterministicStream;
use crate::{CorpusForgeError, Result};

/// Index-based weighted selection table with deterministic integer sampling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedTable {
    cumulative_weights: Vec<u64>,
    total_weight: u64,
}

impl WeightedTable {
    /// Builds a weighted table from integer weights.
    pub fn new(weights: impl IntoIterator<Item = u64>) -> Result<Self> {
        let mut cumulative_weights = Vec::new();
        let mut total_weight = 0_u64;

        for weight in weights {
            total_weight = total_weight.checked_add(weight).ok_or_else(|| {
                CorpusForgeError::invalid_argument(
                    "weighted table total weight overflowed u64 during construction",
                )
            })?;
            cumulative_weights.push(total_weight);
        }

        if cumulative_weights.is_empty() {
            return Err(CorpusForgeError::invalid_argument(
                "weighted table requires at least one entry",
            ));
        }

        if total_weight == 0 {
            return Err(CorpusForgeError::invalid_argument(
                "weighted table requires a non-zero total weight",
            ));
        }

        Ok(Self {
            cumulative_weights,
            total_weight,
        })
    }

    /// Returns the number of entries in the table.
    pub fn len(&self) -> usize {
        self.cumulative_weights.len()
    }

    /// Returns true when the table contains no entries.
    pub fn is_empty(&self) -> bool {
        self.cumulative_weights.is_empty()
    }

    /// Returns the total integer weight.
    pub const fn total_weight(&self) -> u64 {
        self.total_weight
    }

    /// Selects an entry index using the supplied deterministic stream.
    pub fn choose_index(&self, stream: &mut DeterministicStream) -> Result<usize> {
        let target = u64_below(stream, self.total_weight)?;
        let index = self
            .cumulative_weights
            .partition_point(|&cumulative| cumulative <= target);

        Ok(index)
    }
}

fn u64_below(stream: &mut DeterministicStream, bound: u64) -> Result<u64> {
    if bound == 0 {
        return Err(CorpusForgeError::invalid_argument(
            "bounded deterministic stream sampling requires a non-zero bound",
        ));
    }

    let threshold = bound.wrapping_neg() % bound;

    loop {
        let value = stream.next_u64();
        if value >= threshold {
            return Ok(value % bound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WeightedTable;
    use crate::rng::{DeterministicStream, DOMAIN_NGRAM};
    use crate::seed::MasterSeed;
    use crate::CorpusForgeError;
    use corpusforge_testkit::TEST_SEED_BYTES;

    const TEST_SEED: MasterSeed = MasterSeed::from_bytes(TEST_SEED_BYTES);

    #[test]
    fn weighted_table_rejects_empty_weights() {
        let error =
            WeightedTable::new([]).expect_err("empty weighted table should return a project error");

        assert_invalid_argument_contains(error, "at least one entry");
    }

    #[test]
    fn weighted_table_rejects_zero_total_weight() {
        let error = WeightedTable::new([0, 0, 0])
            .expect_err("zero total weight should return a project error");

        assert_invalid_argument_contains(error, "non-zero total weight");
    }

    #[test]
    fn weighted_table_rejects_total_weight_overflow() {
        let error = WeightedTable::new([u64::MAX, 1])
            .expect_err("overflowing total weight should return a project error");

        assert_invalid_argument_contains(error, "overflowed u64");
    }

    #[test]
    fn weighted_table_reports_shape() {
        let table = WeightedTable::new([2, 0, 5]).expect("weights have a non-zero total");

        assert_eq!(table.len(), 3);
        assert!(!table.is_empty());
        assert_eq!(table.total_weight(), 7);
    }

    #[test]
    fn single_entry_weighted_table_always_selects_zero() {
        let table = WeightedTable::new([17]).expect("single positive weight is valid");
        let mut stream = DeterministicStream::from_seed(&TEST_SEED, DOMAIN_NGRAM);

        for _ in 0..32 {
            assert_eq!(
                table
                    .choose_index(&mut stream)
                    .expect("single-entry sampling should succeed"),
                0
            );
        }
    }

    #[test]
    fn weighted_sampling_sequence_is_deterministic() {
        let table = WeightedTable::new([1, 3, 6, 10]).expect("weights have a non-zero total");
        let mut left =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_NGRAM, b"weighted");
        let mut right =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_NGRAM, b"weighted");

        let left_indexes = sample_indexes(&table, &mut left, 16);
        let right_indexes = sample_indexes(&table, &mut right, 16);

        assert_eq!(left_indexes, right_indexes);
        assert_eq!(
            left_indexes,
            [3, 2, 3, 2, 3, 3, 3, 3, 2, 3, 2, 1, 3, 2, 2, 1]
        );
    }

    #[test]
    fn zero_weight_entries_are_never_selected() {
        let table = WeightedTable::new([0, 5, 0]).expect("weights have a non-zero total");
        let mut stream =
            DeterministicStream::from_seed_with_context(&TEST_SEED, DOMAIN_NGRAM, b"zero-slots");

        for _ in 0..64 {
            assert_eq!(
                table
                    .choose_index(&mut stream)
                    .expect("non-zero table sampling should succeed"),
                1
            );
        }
    }

    fn sample_indexes(
        table: &WeightedTable,
        stream: &mut DeterministicStream,
        count: usize,
    ) -> Vec<usize> {
        (0..count)
            .map(|_| {
                table
                    .choose_index(stream)
                    .expect("weighted sampling should succeed")
            })
            .collect()
    }

    fn assert_invalid_argument_contains(error: CorpusForgeError, expected: &str) {
        assert_eq!(error.category(), "invalid_argument");
        assert!(
            error.to_string().contains(expected),
            "expected '{error}' to contain '{expected}'"
        );
    }
}
