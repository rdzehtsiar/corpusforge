// SPDX-License-Identifier: Apache-2.0

//! Core binary envelope for CorpusForge `.cff` v0 profile packs.

use corpusforge_core::{CorpusForgeError, Result};

const MAGIC: [u8; 8] = *b"CFFPACK\0";
const VERSION: u16 = 0;
const HEADER_LEN: usize = 82;
const VERSION_OFFSET: usize = MAGIC.len();
const PAYLOAD_LENGTH_OFFSET: usize = VERSION_OFFSET + 2;
const PAYLOAD_HASH_OFFSET: usize = PAYLOAD_LENGTH_OFFSET + 8;
const PROFILE_HASH_OFFSET: usize = PAYLOAD_HASH_OFFSET + 32;
const PROFILE_HASH_DOMAIN: &[u8] = b"corpusforge.cff.v0.profile\0";

/// A deterministic `.cff` profile pack with canonical file ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfilePack {
    files: Vec<ProfileFile>,
}

impl ProfilePack {
    /// Builds a profile pack from file entries and sorts them by stable path.
    pub fn new(mut files: Vec<ProfileFile>) -> Result<Self> {
        files.sort_by(|left, right| left.path.cmp(&right.path));
        ensure_unique_paths(&files)?;
        Ok(Self { files })
    }

    /// Returns the files in deterministic path order.
    pub fn files(&self) -> &[ProfileFile] {
        &self.files
    }

    /// Serializes this profile pack to the `.cff` v0 binary envelope.
    pub fn to_bytes(&self) -> Vec<u8> {
        write_bytes(self)
    }

    /// Computes the deterministic profile hash as `cff:<blake3>`.
    pub fn profile_hash(&self) -> String {
        profile_hash(self)
    }

    /// Produces an inspect summary for this in-memory pack.
    pub fn inspect(&self) -> InspectSummary {
        let payload = encode_payload(self);
        inspect_payload(self, &payload)
    }

    /// Reads a `.cff` v0 binary envelope into a profile pack.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        read_bytes(bytes)
    }

    /// Verifies a `.cff` v0 binary envelope and returns its inspect summary.
    pub fn verify_bytes(bytes: &[u8]) -> Result<InspectSummary> {
        verify_bytes(bytes)
    }
}

/// A raw file entry stored in a `.cff` profile pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileFile {
    path: String,
    bytes: Vec<u8>,
}

impl ProfileFile {
    /// Builds a file entry with a stable relative path and raw bytes.
    pub fn new(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Result<Self> {
        let path = path.into();
        validate_path(&path)?;

        Ok(Self {
            path,
            bytes: bytes.into(),
        })
    }

    /// Returns the stable relative path for this entry.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the raw bytes stored for this entry.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Human- and machine-readable summary of a verified or in-memory pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectSummary {
    /// `.cff` format version.
    pub version: u16,
    /// Number of file entries in deterministic path order.
    pub file_count: usize,
    /// Total serialized payload length in bytes.
    pub payload_length: u64,
    /// BLAKE3 hash of the serialized payload bytes.
    pub payload_hash: String,
    /// Deterministic profile hash as `cff:<blake3>`.
    pub profile_hash: String,
    /// Sum of raw file byte lengths.
    pub total_file_bytes: u64,
    /// Stable relative paths in deterministic order.
    pub file_paths: Vec<String>,
}

/// Serializes a profile pack to the `.cff` v0 binary envelope.
pub fn write_bytes(pack: &ProfilePack) -> Vec<u8> {
    let payload = encode_payload(pack);
    let payload_hash = blake3::hash(&payload);
    let profile_hash = profile_hash_digest(&payload);

    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(payload_hash.as_bytes());
    bytes.extend_from_slice(profile_hash.as_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

/// Reads and verifies a `.cff` v0 binary envelope into a profile pack.
pub fn read_bytes(bytes: &[u8]) -> Result<ProfilePack> {
    let (_, payload) = read_verified_payload(bytes)?;
    decode_payload(payload)
}

/// Verifies a `.cff` v0 binary envelope and returns an inspect summary.
pub fn verify_bytes(bytes: &[u8]) -> Result<InspectSummary> {
    let (header, payload) = read_verified_payload(bytes)?;
    let pack = decode_payload(payload)?;
    let mut summary = inspect_payload(&pack, payload);
    summary.version = header.version;
    Ok(summary)
}

/// Computes the deterministic profile hash as `cff:<blake3>`.
pub fn profile_hash(pack: &ProfilePack) -> String {
    let payload = encode_payload(pack);
    format!("cff:{}", profile_hash_digest(&payload).to_hex())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Header {
    version: u16,
    payload_length: u64,
    payload_hash: [u8; 32],
    profile_hash: [u8; 32],
}

fn encode_payload(pack: &ProfilePack) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(pack.files.len() as u32).to_le_bytes());

    for file in &pack.files {
        payload.extend_from_slice(&(file.path.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(file.bytes.len() as u64).to_le_bytes());
        payload.extend_from_slice(file.path.as_bytes());
        payload.extend_from_slice(&file.bytes);
    }

    payload
}

fn decode_payload(payload: &[u8]) -> Result<ProfilePack> {
    let mut cursor = Cursor::new(payload);
    let file_count = cursor.read_u32("file count")? as usize;
    let mut files = Vec::with_capacity(file_count);

    for index in 0..file_count {
        let path_len = cursor.read_u32("path length")? as usize;
        let byte_len = cursor.read_u64("file byte length")?;
        let byte_len = usize::try_from(byte_len).map_err(|_| {
            CorpusForgeError::invalid_profile(format!(
                "file entry {index} byte length exceeds this platform; regenerate the profile with a supported size"
            ))
        })?;

        let path_bytes = cursor.read_exact(path_len, "path bytes")?;
        let path = std::str::from_utf8(path_bytes).map_err(|_| {
            CorpusForgeError::invalid_profile(format!(
                "file entry {index} path is not valid UTF-8; rewrite the profile with a stable UTF-8 relative path"
            ))
        })?;

        let bytes = cursor.read_exact(byte_len, "file bytes")?.to_vec();
        files.push(ProfileFile::new(path.to_owned(), bytes)?);
    }

    if cursor.remaining() != 0 {
        return Err(CorpusForgeError::invalid_profile(format!(
            "payload has {} trailing byte(s); rewrite the profile with canonical .cff v0 encoding",
            cursor.remaining()
        )));
    }

    ensure_canonical_order(&files)?;
    Ok(ProfilePack { files })
}

fn read_verified_payload(bytes: &[u8]) -> Result<(Header, &[u8])> {
    let header = read_header(bytes)?;
    let payload_len = usize::try_from(header.payload_length).map_err(|_| {
        CorpusForgeError::invalid_profile(
            "payload length exceeds this platform; use a smaller .cff profile",
        )
    })?;
    let expected_len = HEADER_LEN.checked_add(payload_len).ok_or_else(|| {
        CorpusForgeError::invalid_profile(
            "payload length overflows the envelope size; rewrite the .cff profile",
        )
    })?;

    if bytes.len() < expected_len {
        return Err(CorpusForgeError::invalid_profile(format!(
            "input is truncated while reading payload; expected {expected_len} bytes but found {}; re-copy or regenerate the profile",
            bytes.len()
        )));
    }

    if bytes.len() > expected_len {
        return Err(CorpusForgeError::invalid_profile(format!(
            "input has {} trailing byte(s) after the payload; remove trailing data or rewrite the profile",
            bytes.len() - expected_len
        )));
    }

    let payload = &bytes[HEADER_LEN..expected_len];
    let actual_payload_hash = blake3::hash(payload);
    if actual_payload_hash.as_bytes() != &header.payload_hash {
        return Err(CorpusForgeError::invalid_profile(
            "payload hash mismatch; the .cff profile is corrupted or was edited without rewriting the envelope",
        ));
    }

    let actual_profile_hash = profile_hash_digest(payload);
    if actual_profile_hash.as_bytes() != &header.profile_hash {
        return Err(CorpusForgeError::invalid_profile(
            "profile hash mismatch; the .cff profile is corrupted or was edited without rewriting the envelope",
        ));
    }

    Ok((header, payload))
}

fn read_header(bytes: &[u8]) -> Result<Header> {
    if bytes.len() < HEADER_LEN {
        return Err(CorpusForgeError::invalid_profile(format!(
            "input is truncated while reading .cff header; expected at least {HEADER_LEN} bytes but found {}; re-copy or regenerate the profile",
            bytes.len()
        )));
    }

    if bytes[..MAGIC.len()] != MAGIC {
        return Err(CorpusForgeError::invalid_profile(
            "bad .cff magic; expected a CorpusForge .cff binary envelope",
        ));
    }

    let version = u16::from_le_bytes(read_array(&bytes[VERSION_OFFSET..PAYLOAD_LENGTH_OFFSET]));
    if version != VERSION {
        return Err(CorpusForgeError::unsupported_version(format!(
            ".cff version {version} is unsupported; this reader supports version {VERSION}"
        )));
    }

    let payload_length = u64::from_le_bytes(read_array(
        &bytes[PAYLOAD_LENGTH_OFFSET..PAYLOAD_HASH_OFFSET],
    ));
    let payload_hash = read_array(&bytes[PAYLOAD_HASH_OFFSET..PROFILE_HASH_OFFSET]);
    let profile_hash = read_array(&bytes[PROFILE_HASH_OFFSET..HEADER_LEN]);

    Ok(Header {
        version,
        payload_length,
        payload_hash,
        profile_hash,
    })
}

fn inspect_payload(pack: &ProfilePack, payload: &[u8]) -> InspectSummary {
    InspectSummary {
        version: VERSION,
        file_count: pack.files.len(),
        payload_length: payload.len() as u64,
        payload_hash: blake3::hash(payload).to_hex().to_string(),
        profile_hash: format!("cff:{}", profile_hash_digest(payload).to_hex()),
        total_file_bytes: pack.files.iter().map(|file| file.bytes.len() as u64).sum(),
        file_paths: pack.files.iter().map(|file| file.path.clone()).collect(),
    }
}

fn profile_hash_digest(payload: &[u8]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PROFILE_HASH_DOMAIN);
    hasher.update(payload);
    hasher.finalize()
}

fn ensure_unique_paths(files: &[ProfileFile]) -> Result<()> {
    for pair in files.windows(2) {
        if pair[0].path == pair[1].path {
            return Err(CorpusForgeError::invalid_profile(format!(
                "duplicate profile file path `{}`; each .cff entry needs a unique stable path",
                pair[0].path
            )));
        }
    }

    Ok(())
}

fn ensure_canonical_order(files: &[ProfileFile]) -> Result<()> {
    for pair in files.windows(2) {
        if pair[0].path >= pair[1].path {
            return Err(CorpusForgeError::invalid_profile(
                "file entries are not in canonical path order; rewrite the profile with deterministic .cff v0 serialization",
            ));
        }
    }

    Ok(())
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(CorpusForgeError::invalid_profile(
            "profile file path is empty; use a stable relative path",
        ));
    }

    if path.starts_with('/') || path.starts_with('\\') {
        return Err(CorpusForgeError::invalid_profile(format!(
            "profile file path `{path}` is absolute; use a stable relative path"
        )));
    }

    if path.contains('\\') {
        return Err(CorpusForgeError::invalid_profile(format!(
            "profile file path `{path}` uses backslashes; use forward slashes for stable paths"
        )));
    }

    if path.contains('\0') {
        return Err(CorpusForgeError::invalid_profile(format!(
            "profile file path `{path}` contains a NUL byte; use a stable text path"
        )));
    }

    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(CorpusForgeError::invalid_profile(format!(
                "profile file path `{path}` contains an unstable component; use normalized relative paths"
            )));
        }
    }

    Ok(())
}

fn read_array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut array = [0; N];
    array.copy_from_slice(bytes);
    array
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_u32(&mut self, field: &str) -> Result<u32> {
        let bytes = self.read_exact(4, field)?;
        Ok(u32::from_le_bytes(read_array(bytes)))
    }

    fn read_u64(&mut self, field: &str) -> Result<u64> {
        let bytes = self.read_exact(8, field)?;
        Ok(u64::from_le_bytes(read_array(bytes)))
    }

    fn read_exact(&mut self, len: usize, field: &str) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(len).ok_or_else(|| {
            CorpusForgeError::invalid_profile(format!(
                "payload length overflows while reading {field}; rewrite the profile"
            ))
        })?;

        if end > self.bytes.len() {
            return Err(CorpusForgeError::invalid_profile(format!(
                "payload is truncated while reading {field}; expected {len} byte(s) at offset {} but only {} byte(s) remain",
                self.offset,
                self.remaining()
            )));
        }

        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_pack_round_trips_deterministically() {
        let pack = sample_pack();
        let bytes = pack.to_bytes();

        let decoded = ProfilePack::from_bytes(&bytes).expect("pack should round-trip");

        assert_eq!(decoded, pack);
        assert_eq!(decoded.to_bytes(), bytes);
        assert_eq!(
            decoded.inspect(),
            InspectSummary {
                version: 0,
                file_count: 2,
                payload_length: 68,
                payload_hash: "4ab37c5b23eb947f6c68bef5480e8779e918d0d2136dd9c1723fda80a109419c"
                    .to_owned(),
                profile_hash:
                    "cff:0fc8e99b2577a915f5c2d2ad608d12cd0a615236a86162a9abe10d7c018bb0ec"
                        .to_owned(),
                total_file_bytes: 16,
                file_paths: vec!["alpha.txt".to_owned(), "nested/beta.bin".to_owned()],
            }
        );
    }

    #[test]
    fn stable_hash_for_identical_content() {
        let first = ProfilePack::new(vec![
            ProfileFile::new("z.txt", b"last".to_vec()).expect("valid file"),
            ProfileFile::new("a.txt", b"first".to_vec()).expect("valid file"),
        ])
        .expect("valid pack");
        let second = ProfilePack::new(vec![
            ProfileFile::new("a.txt", b"first".to_vec()).expect("valid file"),
            ProfileFile::new("z.txt", b"last".to_vec()).expect("valid file"),
        ])
        .expect("valid pack");

        assert_eq!(first.profile_hash(), second.profile_hash());
        assert_eq!(profile_hash(&first), second.profile_hash());
    }

    #[test]
    fn deterministic_serialization() {
        let pack = sample_pack();

        assert_eq!(pack.to_bytes(), sample_pack().to_bytes());
    }

    #[test]
    fn unsupported_version_fails_cleanly() {
        let mut bytes = sample_pack().to_bytes();
        bytes[VERSION_OFFSET..PAYLOAD_LENGTH_OFFSET].copy_from_slice(&99_u16.to_le_bytes());

        let error = ProfilePack::from_bytes(&bytes).expect_err("version should fail");

        assert_eq!(error.category(), "unsupported_version");
        assert!(error.to_string().contains("version 99"));
        assert!(error.to_string().contains("supports version 0"));
    }

    #[test]
    fn bad_magic_fails_cleanly() {
        let mut bytes = sample_pack().to_bytes();
        bytes[0] = b'X';

        let error = verify_bytes(&bytes).expect_err("magic should fail");

        assert_eq!(error.category(), "invalid_profile");
        assert!(error.to_string().contains("bad .cff magic"));
    }

    #[test]
    fn truncated_input_fails_cleanly() {
        let bytes = sample_pack().to_bytes();

        let error = read_bytes(&bytes[..HEADER_LEN + 3]).expect_err("truncation should fail");

        assert_eq!(error.category(), "invalid_profile");
        assert!(error.to_string().contains("truncated"));
    }

    #[test]
    fn payload_hash_mismatch_fails_cleanly() {
        let mut bytes = sample_pack().to_bytes();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;

        let error = verify_bytes(&bytes).expect_err("hash mismatch should fail");

        assert_eq!(error.category(), "invalid_profile");
        assert!(error.to_string().contains("payload hash mismatch"));
    }

    fn sample_pack() -> ProfilePack {
        ProfilePack::new(vec![
            ProfileFile::new("nested/beta.bin", vec![0, 159, 146, 169, 255]).expect("valid file"),
            ProfileFile::new("alpha.txt", b"unicode:\xe2\x80\x8d".to_vec()).expect("valid file"),
        ])
        .expect("valid pack")
    }
}
