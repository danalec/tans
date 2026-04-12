use crate::bitstream::{BitReader, BitWriter};
use crate::rans_table::normalize_frequencies;

#[derive(Debug, Clone)]
pub struct StateTable {
    pub table_size: u32,
    pub log2_table_size: u32,
    pub frequencies: Vec<u32>,
    pub cumul: Vec<u32>,
    pub decode_table: Vec<u32>,
}

impl StateTable {
    pub fn new(freqs: &[u32], table_log: u32) -> Self {
        let table_size = 1u32 << table_log;
        assert_eq!(freqs.iter().sum::<u32>(), table_size);

        let mut cumul = vec![0u32];
        for &f in freqs {
            cumul.push(cumul.last().unwrap() + f);
        }

        let decode_table = build_decode_table(freqs, &cumul);

        Self {
            table_size,
            log2_table_size: table_log,
            frequencies: freqs.to_vec(),
            cumul,
            decode_table,
        }
    }

    pub fn from_data(data: &[u32], table_log: u32) -> Self {
        assert!(!data.is_empty());
        let max_sym = *data.iter().max().unwrap() as usize;
        let n_symbols = max_sym + 1;

        let mut counts = vec![0u64; n_symbols];
        for &s in data {
            counts[s as usize] += 1;
        }

        let table_size = 1u32 << table_log;
        let freqs = normalize_frequencies(&counts, table_size);
        Self::new(&freqs, table_log)
    }

    pub fn find_symbol(&self, slot: u32) -> u32 {
        self.decode_table[slot as usize]
    }
}

fn build_decode_table(freqs: &[u32], cumul: &[u32]) -> Vec<u32> {
    let table_size = *cumul.last().unwrap();
    let mut decode_table = vec![0u32; table_size as usize];

    for (sym, &freq) in freqs.iter().enumerate() {
        let start = cumul[sym];
        for i in 0..freq {
            decode_table[(start + i) as usize] = sym as u32;
        }
    }

    decode_table
}

pub struct TableEncoder<'a> {
    table: &'a StateTable,
    state: u32,
}

impl<'a> TableEncoder<'a> {
    pub fn new(table: &'a StateTable) -> Self {
        Self {
            table,
            state: table.table_size,
        }
    }

    pub fn encode(&mut self, sym: u32, writer: &mut BitWriter) {
        let freq = self.table.frequencies[sym as usize];
        let cumul = self.table.cumul[sym as usize];
        let scale_bits = self.table.log2_table_size;

        let max_state = freq << 1;

        while self.state >= max_state {
            writer.write(self.state & 1, 1);
            self.state >>= 1;
        }

        let q = self.state / freq;
        let r = self.state % freq;
        self.state = (q << scale_bits) + r + cumul;
    }

    pub fn finish(&self, writer: &mut BitWriter) {
        writer.write(self.state, self.table.log2_table_size + 1);
    }
}

pub fn table_encode_all(table: &StateTable, data: &[u32]) -> (Vec<u8>, usize) {
    let mut encoder = TableEncoder::new(table);
    let mut writer = BitWriter::new();

    for &sym in data.iter().rev() {
        encoder.encode(sym, &mut writer);
    }

    encoder.finish(&mut writer);
    writer.finish()
}

pub struct TableDecoder<'a> {
    table: &'a StateTable,
    state: u32,
}

impl<'a> TableDecoder<'a> {
    pub fn new(table: &'a StateTable, reader: &mut BitReader) -> Self {
        let init = reader.read(table.log2_table_size + 1);
        Self { table, state: init }
    }

    pub fn decode(&mut self, reader: &mut BitReader) -> u32 {
        let slot = self.state & (self.table.table_size - 1);
        let sym = self.table.find_symbol(slot);

        let freq = self.table.frequencies[sym as usize];
        let cumul = self.table.cumul[sym as usize];
        let scale_bits = self.table.log2_table_size;

        self.state = freq * (self.state >> scale_bits) + slot - cumul;

        while self.state < self.table.table_size {
            let bit = reader.read(1);
            self.state = (self.state << 1) | bit;
        }

        sym
    }
}

pub fn table_decode_all(table: &StateTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    let mut reader = BitReader::from_bytes(bytes, bits);
    let mut decoder = TableDecoder::new(table, &mut reader);
    (0..n).map(|_| decoder.decode(&mut reader)).collect()
}

pub fn table_encode(data: &[u32], log: u32) -> (StateTable, Vec<u8>, usize) {
    let table = StateTable::from_data(data, log);
    let (bytes, bits) = table_encode_all(&table, data);
    (table, bytes, bits)
}

pub fn table_decode(table: &StateTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    table_decode_all(table, bytes, bits, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_table_construction() {
        let freqs = vec![8, 8];
        let table = StateTable::new(&freqs, 4);

        assert_eq!(table.table_size, 16);
        assert_eq!(table.decode_table.len(), 16);
    }

    #[test]
    fn roundtrip_uniform() {
        let data = vec![0u32, 1, 0, 1, 0, 1, 0, 1];
        let (table, encoded, bits) = table_encode(&data, 4);
        let decoded = table_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_single_symbol() {
        let data = vec![0u32; 8];
        let freqs = vec![16u32];
        let table = StateTable::new(&freqs, 4);
        let (encoded, bits) = table_encode_all(&table, &data);
        let decoded = table_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_skewed() {
        let data = vec![0u32, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2];
        let freqs = vec![7u32, 4, 5];
        let table = StateTable::new(&freqs, 4);
        let (encoded, bits) = table_encode_all(&table, &data);
        let decoded = table_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_larger() {
        let data: Vec<u32> = (0..50).map(|i| i % 4).collect();
        let freqs = vec![16u32, 16, 16, 16];
        let table = StateTable::new(&freqs, 6);
        let (encoded, bits) = table_encode_all(&table, &data);
        let decoded = table_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn compression_effectiveness() {
        let mut data = Vec::new();
        for _ in 0..700 {
            data.push(0);
        }
        for _ in 0..200 {
            data.push(1);
        }
        for _ in 0..100 {
            data.push(2);
        }

        let (table, encoded, bits) = table_encode(&data, 10);
        let decoded = table_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
        assert!(bits < data.len() * 8);
    }

    #[test]
    fn decode_table_correctness() {
        let freqs = vec![8u32, 8];
        let table = StateTable::new(&freqs, 4);

        let mut symbol_counts = vec![0u32; 2];
        for &sym in &table.decode_table {
            symbol_counts[sym as usize] += 1;
        }
        assert_eq!(symbol_counts, vec![8, 8]);
    }

    #[test]
    fn compare_with_rans() {
        let data: Vec<u32> = (0..100).map(|i| i % 4).collect();

        let (rans_table, rans_encoded, rans_bits) = crate::encode(&data, 6);
        let (tans_table, tans_encoded, tans_bits) = table_encode(&data, 6);

        let rans_decoded = crate::decode(&rans_table, &rans_encoded, rans_bits, data.len());
        let tans_decoded = table_decode(&tans_table, &tans_encoded, tans_bits, data.len());

        assert_eq!(rans_decoded, data);
        assert_eq!(tans_decoded, data);
    }
}
