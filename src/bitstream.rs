pub struct BitWriter {
    bits: Vec<bool>,
}

impl BitWriter {
    pub fn new() -> Self {
        Self { bits: Vec::new() }
    }

    pub fn write(&mut self, val: u32, n: u32) {
        for i in 0..n {
            self.bits.push((val >> i & 1) == 1);
        }
    }

    pub fn finish(self) -> (Vec<u8>, usize) {
        let mut bytes = Vec::new();
        for chunk in self.bits.chunks(8) {
            let b: u8 = chunk
                .iter()
                .enumerate()
                .filter(|(_, &bit)| bit)
                .map(|(i, _)| 1u8 << i)
                .sum();
            bytes.push(b);
        }
        (bytes, self.bits.len())
    }
}

pub struct BitReader {
    bits: Vec<bool>,
    pos: usize,
}

impl BitReader {
    pub fn from_bytes(bytes: &[u8], total: usize) -> Self {
        let bits: Vec<bool> = bytes
            .iter()
            .flat_map(|&b| (0..8).map(move |i| (b >> i & 1) == 1))
            .take(total)
            .collect();
        Self { bits, pos: total }
    }

    pub fn read(&mut self, n: u32) -> u32 {
        let mut r = 0u32;
        for i in (0..n).rev() {
            if self.pos == 0 {
                break;
            }
            self.pos -= 1;
            if self.bits[self.pos] {
                r |= 1 << i;
            }
        }
        r
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
}
