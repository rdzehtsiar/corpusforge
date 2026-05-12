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

/// A `Write` wrapper that forwards only bytes in `[range_start, range_end)`.
#[derive(Debug)]
pub struct ByteRangeWriter<W> {
    inner: W,
    range_start: u64,
    range_end: u64,
    position: u64,
    bytes_written: u64,
}

impl<W> ByteRangeWriter<W> {
    /// Wraps an inner writer and forwards only bytes in `[range_start, range_end)`.
    ///
    /// Bytes before `range_start` and at or after `range_end` are consumed and discarded.
    pub const fn new(inner: W, range_start: u64, range_end: u64) -> Self {
        Self {
            inner,
            range_start,
            range_end,
            position: 0,
            bytes_written: 0,
        }
    }

    /// Returns the inclusive start of the forwarded range.
    pub const fn range_start(&self) -> u64 {
        self.range_start
    }

    /// Returns the exclusive end of the forwarded range.
    pub const fn range_end(&self) -> u64 {
        self.range_end
    }

    /// Returns the total input byte position consumed by this writer.
    pub const fn position(&self) -> u64 {
        self.position
    }

    /// Returns the number of bytes successfully written to the inner writer.
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

impl<W: Write> Write for ByteRangeWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let input_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        let input_end = self.position.saturating_add(input_len);

        if input_end <= self.range_start || self.position >= self.range_end {
            self.position = input_end;
            return Ok(buf.len());
        }

        let write_start = self
            .range_start
            .saturating_sub(self.position)
            .min(input_len) as usize;
        let write_end = self.range_end.saturating_sub(self.position).min(input_len) as usize;

        if write_start >= write_end {
            self.position = input_end;
            return Ok(buf.len());
        }

        let written = self.inner.write(&buf[write_start..write_end])?;
        self.position = self
            .position
            .saturating_add(u64::try_from(write_start.saturating_add(written)).unwrap_or(u64::MAX));
        self.bytes_written = self.bytes_written.saturating_add(written as u64);

        Ok(write_start.saturating_add(written))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteCountingWriter, ByteRangeWriter, ExactByteLimitWriter};
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

    #[test]
    fn byte_range_writer_discards_prefix_before_writing() {
        let inner = Vec::new();
        let mut writer = ByteRangeWriter::new(inner, 3, 8);

        writer
            .write_all(b"abcdefghij")
            .expect("write should succeed");

        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(writer.position(), 10);
        assert_eq!(writer.into_inner(), b"defgh");
    }

    #[test]
    fn byte_range_writer_writes_exact_range_across_calls() {
        let inner = Vec::new();
        let mut writer = ByteRangeWriter::new(inner, 2, 7);

        writer.write_all(b"ab").expect("prefix should be discarded");
        writer.write_all(b"cde").expect("middle should be written");
        writer
            .write_all(b"fghij")
            .expect("suffix should be discarded");

        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(writer.position(), 10);
        assert_eq!(writer.into_inner(), b"cdefg");
    }

    #[test]
    fn byte_range_writer_empty_range_emits_zero_bytes() {
        let inner = Vec::new();
        let mut writer = ByteRangeWriter::new(inner, 4, 4);

        writer
            .write_all(b"abcdef")
            .expect("empty range should discard");

        assert_eq!(writer.bytes_written(), 0);
        assert_eq!(writer.position(), 6);
        assert!(writer.into_inner().is_empty());
    }

    #[test]
    fn byte_range_writer_preserves_inner_short_write_behavior() {
        let inner = ShortWriter::new(2);
        let mut writer = ByteRangeWriter::new(inner, 3, 8);

        assert_eq!(
            writer.write(b"abcdefghij").expect("write should succeed"),
            5
        );

        assert_eq!(writer.bytes_written(), 2);
        assert_eq!(writer.position(), 5);
        assert_eq!(writer.into_inner().bytes, b"de");
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
