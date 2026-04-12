use crate::bitstream::{BitReader, BitWriter};
use crate::rans_table::normalize_frequencies;

#[derive(Debug, Clone)]
pub struct TansTable {
    pub table_size: u32,
    pub log2_table_size: u32,
    pub frequencies: Vec<u32>,
    pub cumul: Vec<u32>,
    pub decode_table: Vec<DecodeEntry>,
    pub encode_table: Vec<EncodeEntry>,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct DecodeEntry {
    pub symbol: u8,
    pub nb_bits: u8,
    pub new_state: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct EncodeEntry {
    pub nb_bits: u8,
    pub new_state: u16,
}

impl TansTable {
    pub fn new(freqs: &[u32], table_log: u32) -> Self {
        let table_size = 1u32 << table_log;
        assert_eq!(freqs.iter().sum::<u32>(), table_size);
        assert!(freqs.iter().all(|&f| f > 0));

        let mut cumul = vec![0u32];
        for &f in freqs {
            cumul.push(cumul.last().unwrap() + f);
        }

        let (decode_table, encode_table) = Self::build_tables(freqs, &cumul, table_log);

        Self {
            table_size,
            log2_table_size: table_log,
            frequencies: freqs.to_vec(),
            cumul,
            decode_table,
            encode_table,
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

    fn build_tables(
        freqs: &[u32],
        cumul: &[u32],
        table_log: u32,
    ) -> (Vec<DecodeEntry>, Vec<EncodeEntry>) {
        let table_size = 1u32 << table_log;

        let mut decode_table: Vec<DecodeEntry> = vec![
            DecodeEntry {
                symbol: 0,
                nb_bits: 0,
                new_state: 0,
            };
            table_size as usize
        ];

        let total_entries: usize = freqs.iter().map(|&f| f as usize).sum();
        let mut encode_table: Vec<EncodeEntry> = vec![
            EncodeEntry {
                nb_bits: 0,
                new_state: 0,
            };
            total_entries
        ];

        for (sym, &freq) in freqs.iter().enumerate() {
            let base = cumul[sym];
            for i in 0..freq {
                let next_state = freq + i;
                let ilog2_next = if next_state > 0 {
                    31u32 - next_state.leading_zeros()
                } else {
                    0
                };
                let nb_bits = if next_state < table_size {
                    table_log.saturating_sub(ilog2_next)
                } else {
                    0
                };

                let new_state = (next_state << nb_bits) - table_size;

                let pos = (base + i) as usize;
                decode_table[pos] = DecodeEntry {
                    symbol: sym as u8,
                    nb_bits: nb_bits as u8,
                    new_state: new_state as u16,
                };

                encode_table[pos] = EncodeEntry {
                    nb_bits: nb_bits as u8,
                    new_state: new_state as u16,
                };
            }
        }

        (decode_table, encode_table)
    }
}

pub struct TansEncoder<'a> {
    table: &'a TansTable,
    state: u32,
}

impl<'a> TansEncoder<'a> {
    pub fn new(table: &'a TansTable) -> Self {
        Self {
            table,
            state: table.table_size,
        }
    }

    pub fn encode(&mut self, sym: u32, writer: &mut BitWriter) {
        let freq = self.table.frequencies[sym as usize];
        let cumul = self.table.cumul[sym as usize];

        while self.state >= freq * 2 {
            writer.write(self.state & 1, 1);
            self.state >>= 1;
        }

        let slot = cumul + (self.state - freq);
        self.state = self.table.table_size + slot;
    }

    pub fn finish(&self, writer: &mut BitWriter) {
        writer.write(self.state, self.table.log2_table_size + 1);
    }
}

pub fn tans_encode_all(table: &TansTable, data: &[u32]) -> (Vec<u8>, usize) {
    let mut encoder = TansEncoder::new(table);
    let mut writer = BitWriter::new();

    for &sym in data.iter().rev() {
        encoder.encode(sym, &mut writer);
    }

    encoder.finish(&mut writer);
    writer.finish()
}

pub struct TansDecoder<'a> {
    table: &'a TansTable,
    state: u32,
}

impl<'a> TansDecoder<'a> {
    pub fn new(table: &'a TansTable, reader: &mut BitReader) -> Self {
        let init = reader.read(table.log2_table_size + 1);
        Self { table, state: init }
    }

    pub fn decode(&mut self, reader: &mut BitReader) -> u32 {
        let slot = self.state & (self.table.table_size - 1);
        let entry = self.table.decode_table[slot as usize];

        let symbol = entry.symbol as u32;
        let freq = self.table.frequencies[symbol as usize];
        let cumul = self.table.cumul[symbol as usize];

        self.state = freq + (slot - cumul);

        while self.state < self.table.table_size {
            let bit = reader.read(1);
            self.state = (self.state << 1) | bit;
        }

        symbol
    }
}

pub fn tans_decode_all(table: &TansTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    let mut reader = BitReader::from_bytes(bytes, bits);
    let mut decoder = TansDecoder::new(table, &mut reader);
    (0..n).map(|_| decoder.decode(&mut reader)).collect()
}

pub fn tans_encode(data: &[u32], log: u32) -> (TansTable, Vec<u8>, usize) {
    let table = TansTable::from_data(data, log);
    let (bytes, bits) = tans_encode_all(&table, data);
    (table, bytes, bits)
}

pub fn tans_decode(table: &TansTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    tans_decode_all(table, bytes, bits, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_table_construction() {
        let freqs = vec![8, 8];
        let table = TansTable::new(&freqs, 4);

        assert_eq!(table.table_size, 16);
        assert_eq!(table.decode_table.len(), 16);
        assert_eq!(table.encode_table.len(), 16);
    }

    #[test]
    fn roundtrip_uniform() {
        let data = vec![0u32, 1, 0, 1, 0, 1, 0, 1];
        let (table, encoded, bits) = tans_encode(&data, 4);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_single_symbol() {
        let data = vec![0u32; 8];
        let freqs = vec![16u32];
        let table = TansTable::new(&freqs, 4);
        let (encoded, bits) = tans_encode_all(&table, &data);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_skewed() {
        let data = vec![0u32, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2];
        let freqs = vec![7u32, 4, 5];
        let table = TansTable::new(&freqs, 4);
        let (encoded, bits) = tans_encode_all(&table, &data);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_larger() {
        let data: Vec<u32> = (0..50).map(|i| i % 4).collect();
        let freqs = vec![16u32, 16, 16, 16];
        let table = TansTable::new(&freqs, 6);
        let (encoded, bits) = tans_encode_all(&table, &data);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
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

        let (table, encoded, bits) = tans_encode(&data, 10);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
        assert!(bits < data.len() * 8);
    }

    #[test]
    fn decode_table_correctness() {
        let freqs = vec![8u32, 8];
        let table = TansTable::new(&freqs, 4);

        let mut symbol_counts = vec![0u32; 2];
        for entry in &table.decode_table {
            symbol_counts[entry.symbol as usize] += 1;
        }
        assert_eq!(symbol_counts, vec![8, 8]);
    }

    #[test]
    fn compare_with_rans() {
        let data: Vec<u32> = (0..100).map(|i| i % 4).collect();

        let (rans_table, rans_encoded, rans_bits) = crate::encode(&data, 6);
        let (tans_table, tans_encoded, tans_bits) = tans_encode(&data, 6);

        let rans_decoded = crate::decode(&rans_table, &rans_encoded, rans_bits, data.len());
        let tans_decoded = tans_decode(&tans_table, &tans_encoded, tans_bits, data.len());

        assert_eq!(rans_decoded, data);
        assert_eq!(tans_decoded, data);
    }

    #[test]
    fn roundtrip_auto_freqs() {
        let data: Vec<u32> = (0..200).map(|i| i % 8).collect();
        let (table, encoded, bits) = tans_encode(&data, 8);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }
}
