// SPDX-License-Identifier: Apache-2.0

//! Streaming output helpers for deterministic byte emission.

use std::io::{self, Write};

/// A `Write` wrapper that counts bytes successfully written to the inner writer.
#[derive(Debug)]
pub struct ByteCountingWriter<W> {
    inner: W,
    bytes_written: u64,
}

impl<W> ByteCountingWriter<W> {
    /// Wraps an inner writer and starts the byte count at zero.
    pub const fn new(inner: W) -> Self {
        Self {
            inner,
            bytes_written: 0,
        }
    }

    /// Returns the number of bytes successfully reported as written.
    pub const fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Returns a shared reference to the inner writer.
    pub const fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Returns a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Unwraps the writer and returns the inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for ByteCountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes_written = self.bytes_written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A `Write` wrapper that permits exactly up to a configured byte limit.
#[derive(Debug)]
pub struct ExactByteLimitWriter<W> {
    inner: W,
    limit: u64,
    bytes_written: u64,
}

impl<W> ExactByteLimitWriter<W> {
    /// Wraps an inner writer and caps future writes at `limit` bytes.
    pub const fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            limit,
            bytes_written: 0,
        }
    }

    /// Returns the configured byte limit.
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Returns the number of bytes successfully written through the cap.
    pub const fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Returns the remaining writable byte count.
    pub const fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.bytes_written)
    }

    /// Returns a shared reference to the inner writer.
    pub const fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Returns a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Unwraps the writer and returns the inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for ExactByteLimitWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let remaining = self.remaining();
        if remaining == 0 {
            return Ok(0);
        }

        let permitted =
            usize::try_from(remaining).map_or(buf.len(), |remaining| remaining.min(buf.len()));
        let written = self.inner.write(&buf[..permitted])?;
        self.bytes_written = self.bytes_written.saturating_add(written as u64);

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteCountingWriter, ExactByteLimitWriter};
    use std::io::{self, Write};

    #[test]
    fn byte_counting_writer_counts_successful_writes() {
        let inner = Vec::new();
        let mut writer = ByteCountingWriter::new(inner);

        assert_eq!(writer.write(b"abc").expect("write should succeed"), 3);
        assert_eq!(writer.write(b"de").expect("write should succeed"), 2);
        writer.flush().expect("flush should succeed");

        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(writer.into_inner(), b"abcde");
    }

    #[test]
    fn exact_byte_limit_accepts_short_writes() {
        let inner = Vec::new();
        let mut writer = ExactByteLimitWriter::new(inner, 8);

        assert_eq!(writer.write(b"abc").expect("write should succeed"), 3);

        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(writer.remaining(), 5);
        assert_eq!(writer.into_inner(), b"abc");
    }

    #[test]
    fn exact_byte_limit_reports_partial_oversized_write() {
        let inner = Vec::new();
        let mut writer = ExactByteLimitWriter::new(inner, 5);

        assert_eq!(writer.write(b"abcdefgh").expect("write should succeed"), 5);
        assert_eq!(writer.write(b"z").expect("cap is exhausted"), 0);

        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(writer.remaining(), 0);
        assert_eq!(writer.into_inner(), b"abcde");
    }

    #[test]
    fn exact_byte_limit_zero_cap_does_not_write() {
        let inner = Vec::new();
        let mut writer = ExactByteLimitWriter::new(inner, 0);

        assert_eq!(
            writer.write(b"abc").expect("zero cap is not an I/O error"),
            0
        );

        assert_eq!(writer.bytes_written(), 0);
        assert_eq!(writer.remaining(), 0);
        assert!(writer.into_inner().is_empty());
    }

    #[test]
    fn exact_byte_limit_preserves_inner_short_write_behavior() {
        let inner = ShortWriter::new(2);
        let mut writer = ExactByteLimitWriter::new(inner, 5);

        assert_eq!(writer.write(b"abcdef").expect("write should succeed"), 2);

        assert_eq!(writer.bytes_written(), 2);
        assert_eq!(writer.remaining(), 3);
        assert_eq!(writer.into_inner().bytes, b"ab");
    }

    struct ShortWriter {
        bytes: Vec<u8>,
        max_per_write: usize,
    }

    impl ShortWriter {
        fn new(max_per_write: usize) -> Self {
            Self {
                bytes: Vec::new(),
                max_per_write,
            }
        }
    }

    impl Write for ShortWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let permitted = self.max_per_write.min(buf.len());
            self.bytes.extend_from_slice(&buf[..permitted]);
            Ok(permitted)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
