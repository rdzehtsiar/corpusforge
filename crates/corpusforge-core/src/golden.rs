// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::rng::{DeterministicStream, DOMAIN_NGRAM, DOMAIN_UNICODE};
    use crate::seed::MasterSeed;
    use crate::weighted::WeightedTable;
    use corpusforge_testkit::bytes_to_hex;
    use std::str::FromStr;

    const STREAM_BYTES: usize = 32;
    const WEIGHTED_SAMPLE_COUNT: usize = 16;

    #[test]
    fn seed_1337_ngram_stream_matches_golden_hex() {
        let mut stream = DeterministicStream::from_seed(&seed_1337(), DOMAIN_NGRAM);
        let mut bytes = [0_u8; STREAM_BYTES];
        stream.fill_bytes(&mut bytes);

        assert_eq!(bytes_to_hex(&bytes), fixture("seed_1337_stream_ngram.hex"));
    }

    #[test]
    fn seed_1337_unicode_stream_matches_golden_hex() {
        let mut stream = DeterministicStream::from_seed(&seed_1337(), DOMAIN_UNICODE);
        let mut bytes = [0_u8; STREAM_BYTES];
        stream.fill_bytes(&mut bytes);

        assert_eq!(
            bytes_to_hex(&bytes),
            fixture("seed_1337_stream_unicode.hex")
        );
    }

    #[test]
    fn seed_1337_weighted_choice_sequence_matches_golden() {
        let table = WeightedTable::new([1, 3, 6, 10]).expect("weights have a non-zero total");
        let mut stream =
            DeterministicStream::from_seed_with_context(&seed_1337(), DOMAIN_NGRAM, b"weighted");

        let sequence: Vec<usize> = (0..WEIGHTED_SAMPLE_COUNT)
            .map(|_| {
                table
                    .choose_index(&mut stream)
                    .expect("weighted sampling should succeed")
            })
            .collect();

        assert_eq!(
            format!("{sequence:?}"),
            fixture("seed_1337_weighted_choice_sequence.txt")
        );
    }

    fn seed_1337() -> MasterSeed {
        MasterSeed::from_str("1337").expect("integer seed 1337 should parse")
    }

    fn fixture(name: &str) -> &'static str {
        match name {
            "seed_1337_stream_ngram.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_stream_ngram.hex"
            ))
            .trim(),
            "seed_1337_stream_unicode.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_stream_unicode.hex"
            ))
            .trim(),
            "seed_1337_weighted_choice_sequence.txt" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/golden/seed_1337_weighted_choice_sequence.txt"
            ))
            .trim(),
            _ => panic!("unknown golden fixture '{name}'"),
        }
    }
}
