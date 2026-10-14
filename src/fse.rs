use crate::bitstream::{BitReader, BitWriter};
use crate::rans_table::{normalize_frequencies, MAX_TABLE_LOG};

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
    pub symbol: u32,
    pub nb_bits: u8,
    pub new_state: u32,
}

impl FseTable {
    pub fn new(freqs: &[u32], table_log: u32) -> Self {
        assert!(
            table_log <= MAX_TABLE_LOG,
            "table_log must be <= {MAX_TABLE_LOG} (got {table_log})"
        );
        let table_size = 1u32 << table_log;
        assert_eq!(
            freqs.iter().sum::<u32>(),
            table_size,
            "frequencies must sum to the table size ({table_size})"
        );

        let mut cumul = vec![0u32];
        for &f in freqs {
            cumul.push(cumul.last().unwrap() + f);
        }

        let decode_table = Self::build_decode_table(freqs, table_log);

        Self {
            table_size,
            log2_table_size: table_log,
            frequencies: freqs.to_vec(),
            cumul,
            decode_table,
        }
    }

    pub fn from_data(data: &[u32], table_log: u32) -> Self {
        assert!(!data.is_empty(), "cannot build a table from an empty input");
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

    /// Entries are laid out in symbol order, so the slot a symbol occupies starts
    /// at its cumulative frequency.
    fn build_decode_table(freqs: &[u32], table_log: u32) -> Vec<DecodeEntry> {
        let table_size = 1u32 << table_log;
        let mut decode_table = Vec::with_capacity(table_size as usize);

        for (sym, &freq) in freqs.iter().enumerate() {
            for i in 0..freq {
                // `next_state` is the encoder state this slot decodes back to.
                let next_state = freq + i;
                let nb_bits = if next_state < table_size {
                    table_log - (31 - next_state.leading_zeros())
                } else {
                    0
                };
                let new_state = (next_state << nb_bits) - table_size;

                decode_table.push(DecodeEntry {
                    symbol: sym as u32,
                    nb_bits: nb_bits as u8,
                    new_state,
                });
            }
        }

        debug_assert_eq!(decode_table.len(), table_size as usize);
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
        assert!(
            freq > 0,
            "symbol {sym} has frequency 0 and cannot be encoded"
        );
        let cumul = self.table.cumul[sym as usize];
        let scale_bits = self.table.log2_table_size;

        // Renormalize: keep state in range [freq, 2*freq)
        while self.state >= (freq << 1) {
            writer.write(self.state & 1, 1);
            self.state >>= 1;
        }

        // rANS update: `(state / freq) << log + state % freq + cumul`. With the
        // state in [freq, 2*freq) the quotient is always 1 and the remainder is
        // `state - freq`, so it collapses to the slot index.
        self.state = (1 << scale_bits) + cumul + (self.state - freq);
        debug_assert_eq!(self.state >> scale_bits, 1);
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

        // The previous state is `(next_state << nb_bits)` with `nb_bits` fresh bits
        // appended; the table stores that value minus the table size.
        self.state = entry.new_state + self.table.table_size + reader.read(entry.nb_bits as u32);

        entry.symbol
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

    /// The bug this module shipped with: the decoder shifted the reconstructed
    /// base state instead of appending bits to it, which only stayed invisible
    /// while every entry had `nb_bits == 0`.
    #[test]
    fn roundtrip_multiple_symbols() {
        let data: Vec<u32> = (0..500).map(|i| (i * 7) % 5).collect();
        let (table, encoded, bits) = fse_encode(&data, 10);
        let decoded = fse_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_across_table_sizes() {
        let data: Vec<u32> = (0..400).map(|i| (i * 7) % 5).collect();
        for log in 4..=12u32 {
            let (table, encoded, bits) = fse_encode(&data, log);
            let decoded = fse_decode(&table, &encoded, bits, data.len());
            assert_eq!(decoded, data, "table_log = {log}");
        }
    }

    #[test]
    fn roundtrip_with_symbols_above_255() {
        let data: Vec<u32> = (0..900).map(|i| (i * 13) % 300).collect();
        let (table, encoded, bits) = fse_encode(&data, 12);
        let decoded = fse_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_with_a_table_larger_than_65536_entries() {
        let data: Vec<u32> = (0..900).map(|i| (i * 13) % 17).collect();
        let (table, encoded, bits) = fse_encode(&data, 20);
        let decoded = fse_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn compression_effectiveness() {
        let mut data = vec![0u32; 700];
        data.extend(std::iter::repeat_n(1u32, 200));
        data.extend(std::iter::repeat_n(2u32, 100));

        let (table, encoded, bits) = fse_encode(&data, 10);
        let decoded = fse_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
        assert!(bits < data.len() * 8);
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

    #[test]
    fn absent_symbols_keep_their_slot_free() {
        let table = FseTable::new(&[12, 0, 4], 4);
        assert_eq!(table.decode_table.len(), 16);
        assert!(table.decode_table.iter().all(|e| e.symbol != 1));
    }

    #[test]
    #[should_panic(expected = "frequency 0")]
    fn encoding_an_absent_symbol_fails_instead_of_hanging() {
        let table = FseTable::new(&[12, 0, 4], 4);
        let mut encoder = FseEncoder::new(&table);
        encoder.encode(1, &mut BitWriter::new());
    }
}
