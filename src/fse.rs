use crate::bitstream::{BitReader, BitWriter};
use crate::rans_table::normalize_frequencies;

#[derive(Debug, Clone)]
pub struct FseTable {
    pub table_size: u32,
    pub log2_table_size: u32,
    pub frequencies: Vec<u32>,
    pub cumul: Vec<u32>,
    pub decode_table: Vec<DecodeEntry>,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct DecodeEntry {
    pub symbol: u8,
    pub nb_bits: u8,
    pub new_state: u16,
}

impl FseTable {
    pub fn new(freqs: &[u32], table_log: u32) -> Self {
        let table_size = 1u32 << table_log;
        assert_eq!(freqs.iter().sum::<u32>(), table_size);
        assert!(freqs.iter().all(|&f| f > 0));

        let mut cumul = vec![0u32];
        for &f in freqs {
            cumul.push(cumul.last().unwrap() + f);
        }

        let decode_table = Self::build_decode_table(freqs, &cumul, table_log);

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

    fn build_decode_table(freqs: &[u32], cumul: &[u32], table_log: u32) -> Vec<DecodeEntry> {
        let table_size = 1u32 << table_log;
        let mut decode_table: Vec<DecodeEntry> = Vec::with_capacity(table_size as usize);

        for (sym, &freq) in freqs.iter().enumerate() {
            let _start = cumul[sym];
            for i in 0..freq {
                let next_state = freq + i;
                let nb_bits = table_log - (31 - next_state.leading_zeros());
                let new_state = (next_state << nb_bits) - table_size;

                decode_table.push(DecodeEntry {
                    symbol: sym as u8,
                    nb_bits: nb_bits as u8,
                    new_state: new_state as u16,
                });
            }
        }

        decode_table
    }
}

pub struct FseEncoder<'a> {
    table: &'a FseTable,
    state: u32,
}

impl<'a> FseEncoder<'a> {
    pub fn new(table: &'a FseTable) -> Self {
        Self {
            table,
            state: table.table_size,
        }
    }

    pub fn encode(&mut self, sym: u32, writer: &mut BitWriter) {
        let freq = self.table.frequencies[sym as usize];
        let cumul = self.table.cumul[sym as usize];
        let scale_bits = self.table.log2_table_size;

        // Renormalize: keep state in range [freq, 2*freq)
        while self.state >= (freq << 1) {
            writer.write(self.state & 1, 1);
            self.state >>= 1;
        }

        // Encode using rANS formula
        let q = self.state / freq;
        let r = self.state % freq;
        self.state = (q << scale_bits) + r + cumul;
    }

    pub fn finish(&self, writer: &mut BitWriter) {
        writer.write(self.state, self.table.log2_table_size + 1);
    }
}

pub fn fse_encode_all(table: &FseTable, data: &[u32]) -> (Vec<u8>, usize) {
    let mut encoder = FseEncoder::new(table);
    let mut writer = BitWriter::new();

    for &sym in data.iter().rev() {
        encoder.encode(sym, &mut writer);
    }

    encoder.finish(&mut writer);
    writer.finish()
}

pub struct FseDecoder<'a> {
    table: &'a FseTable,
    state: u32,
}

impl<'a> FseDecoder<'a> {
    pub fn new(table: &'a FseTable, reader: &mut BitReader) -> Self {
        let init = reader.read(table.log2_table_size + 1);
        Self { table, state: init }
    }

    pub fn decode(&mut self, reader: &mut BitReader) -> u32 {
        let slot = self.state & (self.table.table_size - 1);
        let entry = self.table.decode_table[slot as usize];

        let symbol = entry.symbol as u32;

        self.state = entry.new_state as u32;
        for _ in 0..entry.nb_bits {
            let bit = reader.read(1);
            self.state = (self.state << 1) | bit;
        }

        symbol
    }
}

pub fn fse_decode_all(table: &FseTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    let mut reader = BitReader::from_bytes(bytes, bits);
    let mut decoder = FseDecoder::new(table, &mut reader);
    (0..n).map(|_| decoder.decode(&mut reader)).collect()
}

pub fn fse_encode(data: &[u32], log: u32) -> (FseTable, Vec<u8>, usize) {
    let table = FseTable::from_data(data, log);
    let (bytes, bits) = fse_encode_all(&table, data);
    (table, bytes, bits)
}

pub fn fse_decode(table: &FseTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    fse_decode_all(table, bytes, bits, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_table_construction() {
        let freqs = vec![8, 8];
        let table = FseTable::new(&freqs, 4);

        assert_eq!(table.table_size, 16);
        assert_eq!(table.decode_table.len(), 16);
    }

    #[test]
    fn roundtrip_single_symbol() {
        let data = vec![0u32; 8];
        let freqs = vec![16u32];
        let table = FseTable::new(&freqs, 4);
        let (encoded, bits) = fse_encode_all(&table, &data);
        let decoded = fse_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_table_correctness() {
        let freqs = vec![8u32, 8];
        let table = FseTable::new(&freqs, 4);

        let mut symbol_counts = vec![0u32; 2];
        for entry in &table.decode_table {
            symbol_counts[entry.symbol as usize] += 1;
        }
        assert_eq!(symbol_counts, vec![8, 8]);
    }
}
