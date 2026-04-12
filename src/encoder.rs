use crate::bitstream::BitWriter;
use crate::rans_table::RansTable;

pub struct Encoder {
    state: u32,
}

impl Encoder {
    pub fn new(table: &RansTable) -> Self {
        Self {
            state: table.table_size,
        }
    }

    pub fn encode(&mut self, table: &RansTable, sym: u32, writer: &mut BitWriter) {
        #[cfg(test)]
        println!("encode: state={}, symbol={}", self.state, sym);

        let f = table.frequencies[sym as usize];
        let scale_bits = table.log2_table_size;

        let x_max = f << 1;

        while self.state >= x_max {
            writer.write(self.state & 1, 1);
            #[cfg(test)]
            println!("  wrote bit {}", self.state & 1);
            self.state >>= 1;
        }

        #[cfg(test)]
        println!("  after renorm, state={}", self.state);

        let start = table.cumul[sym as usize];
        let q = self.state / f;
        let r = self.state % f;
        self.state = (q << scale_bits) + r + start;

        #[cfg(test)]
        println!("  after encode, state={}", self.state);
    }

    pub fn finish(&self, table: &RansTable, writer: &mut BitWriter) {
        #[cfg(test)]
        println!("flush: final state={}", self.state);
        writer.write(self.state, table.log2_table_size + 1);
    }
}

pub fn encode_all(table: &RansTable, data: &[u32]) -> (Vec<u8>, usize) {
    let mut enc = Encoder::new(table);
    let mut w = BitWriter::new();

    for &sym in data.iter().rev() {
        enc.encode(table, sym, &mut w);
    }
    enc.finish(table, &mut w);
    w.finish()
}

#[derive(Clone, Copy)]
pub struct Config {
    pub log: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self { log: 12 }
    }
}

pub fn encode_with_stats(data: &[u32], cfg: Config) -> (RansTable, Vec<u8>, usize) {
    assert!(!data.is_empty());
    let max_sym = *data.iter().max().unwrap();
    let n = (max_sym + 1) as usize;

    let mut counts = vec![0u64; n];
    for &s in data {
        counts[s as usize] += 1;
    }

    let size = 1u32 << cfg.log;
    let freqs = crate::rans_table::normalize_frequencies(&counts, size);
    let table = RansTable::new(&freqs, cfg.log);
    let (bytes, bits) = encode_all(&table, data);
    (table, bytes, bits)
}
