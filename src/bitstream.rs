pub struct BitWriter {
    acc: u64,
    acc_bits: u32,
    bytes: Vec<u8>,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            acc: 0,
            acc_bits: 0,
            bytes: Vec::new(),
        }
    }

    /// Appends the low `n` bits of `val`, least significant bit first.
    pub fn write(&mut self, val: u32, n: u32) {
        assert!(n <= 32, "cannot write more than 32 bits at once (got {n})");
        if n == 0 {
            return;
        }
        self.acc |= (val as u64) << self.acc_bits;
        self.acc_bits += n;
        while self.acc_bits >= 8 {
            self.bytes.push(self.acc as u8);
            self.acc >>= 8;
            self.acc_bits -= 8;
        }
    }

    /// Flushes the partial byte and returns the buffer with the exact bit count.
    pub fn finish(mut self) -> (Vec<u8>, usize) {
        let total_bits = self.bytes.len() * 8 + self.acc_bits as usize;
        if self.acc_bits > 0 {
            self.bytes.push(self.acc as u8);
        }
        (self.bytes, total_bits)
    }
}

pub struct BitReader {
    words: Vec<u64>,
    pos: usize,
}

impl BitReader {
    /// Loads `total` bits from `bytes`, reading them back in reverse write order.
    pub fn from_bytes(bytes: &[u8], total: usize) -> Self {
        let words: Vec<u64> = bytes
            .chunks(8)
            .map(|chunk| {
                let mut w = 0u64;
                for (i, &b) in chunk.iter().enumerate() {
                    w |= (b as u64) << (8 * i);
                }
                w
            })
            .collect();
        // Padding bits in the last byte are never meaningful; a `total` larger
        // than the buffer is clamped so a lying header cannot index past `words`.
        let pos = total.min(bytes.len() * 8);
        Self { words, pos }
    }

    /// Reads `n` bits, newest first, rebuilding the value written by [`BitWriter::write`].
    pub fn read(&mut self, n: u32) -> u32 {
        assert!(n <= 32, "cannot read more than 32 bits at once (got {n})");
        if n == 0 {
            return 0;
        }
        let n = n as usize;
        let start = self.pos.saturating_sub(n);
        let available = self.pos - start;
        self.pos = start;
        if available == 0 {
            return 0;
        }

        let mut value = 0u64;
        let mut filled = 0usize;
        let mut bit = start;
        while filled < available {
            let word = self.words[bit >> 6];
            let offset = (bit & 63) as u32;
            let take = (64 - offset).min((available - filled) as u32);
            let mask = (1u64 << take) - 1;
            value |= ((word >> offset) & mask) << filled;
            filled += take as usize;
            bit += take as usize;
        }

        // A short stream leaves the missing low bits zero, matching a reader that
        // ran out of input mid-value.
        (value << (n - available)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ans_roundtrip() {
        let mut w = BitWriter::new();
        w.write(0b101, 3);
        w.write(0b1101, 4);
        w.write(26, 5);
        let (bytes, bits) = w.finish();

        let mut r = BitReader::from_bytes(&bytes, bits);
        assert_eq!(r.read(5), 26);
        assert_eq!(r.read(4), 0b1101);
        assert_eq!(r.read(3), 0b101);
    }

    #[test]
    fn byte_aligned_streams_have_no_trailing_byte() {
        let mut w = BitWriter::new();
        for _ in 0..3 {
            w.write(0xff, 8);
        }
        let (bytes, bits) = w.finish();
        assert_eq!(bits, 24);
        assert_eq!(bytes.len(), 3);
    }

    #[test]
    fn partial_byte_is_padded_with_zeroes() {
        let mut w = BitWriter::new();
        w.write(0b101, 3);
        let (bytes, bits) = w.finish();
        assert_eq!(bits, 3);
        assert_eq!(bytes, vec![0b101]);
    }

    #[test]
    fn every_value_width_roundtrips() {
        for width in 1..=32u32 {
            let val: u32 = if width == 32 {
                u32::MAX
            } else {
                (1u32 << width) - 1
            };
            let mut w = BitWriter::new();
            w.write(val, width);
            w.write(val, width);
            let (bytes, bits) = w.finish();

            let mut r = BitReader::from_bytes(&bytes, bits);
            assert_eq!(r.read(width), val, "width {width}");
            assert_eq!(r.read(width), val, "width {width}");
        }
    }

    #[test]
    fn reading_past_the_stream_yields_zeroes_without_panicking() {
        let mut w = BitWriter::new();
        w.write(0b111, 3);
        let (bytes, bits) = w.finish();

        let mut r = BitReader::from_bytes(&bytes, bits);
        // 4 bits requested, only 3 available: the missing low bits read as zero.
        assert_eq!(r.read(4), 0b1110);
        assert_eq!(r.read(4), 0);
    }

    #[test]
    fn total_larger_than_buffer_is_clamped() {
        // A header claiming 64 bits must not index past the 1-byte payload: the
        // 8 real bits land in the high half of the first read, the rest read zero.
        let mut r = BitReader::from_bytes(&[0b1010_1010], 64);
        assert_eq!(r.read(32), 0xAA00_0000);
        assert_eq!(r.read(32), 0);
    }

    #[test]
    #[should_panic(expected = "cannot write more than 32 bits")]
    fn write_rejects_more_than_32_bits() {
        BitWriter::new().write(1, 33);
    }

    #[test]
    #[should_panic(expected = "cannot read more than 32 bits")]
    fn read_rejects_more_than_32_bits() {
        BitReader::from_bytes(&[0], 8).read(33);
    }
}
