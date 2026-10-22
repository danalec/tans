// SPDX-License-Identifier: LicenseRef-tans-1.0
// Copyright (c) 2026 Dan Alec Yamaguchi
//
// Licensed under the tANS License in LICENSE: the MIT License and the Apache
// License 2.0 apply in full, and commercial use is restricted.

use crate::bitstream::{BitReader, BitWriter};
use crate::rans_table::{normalize_frequencies, MAX_TABLE_LOG};

#[derive(Debug, Clone)]
pub struct TansTable {
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

impl TansTable {
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

    /// One entry per table slot: which symbol lives there, how many bits the
    /// decoder has to pull for the previous state, and where that state starts.
    fn build_decode_table(freqs: &[u32], cumul: &[u32], table_log: u32) -> Vec<DecodeEntry> {
        let table_size = 1u32 << table_log;
        let mut decode_table = vec![DecodeEntry::default(); table_size as usize];

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

                decode_table[(cumul[sym] + i) as usize] = DecodeEntry {
                    symbol: sym as u32,
                    nb_bits: nb_bits as u8,
                    new_state,
                };
            }
        }

        decode_table
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
        assert!(
            freq > 0,
            "symbol {sym} has frequency 0 and cannot be encoded"
        );
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

        // `new_state` is the previous state minus the table size, so restoring it
        // is one table lookup plus one multi-bit read.
        self.state = entry.new_state + self.table.table_size + reader.read(entry.nb_bits as u32);

        entry.symbol
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
    fn roundtrip_across_table_sizes() {
        let data: Vec<u32> = (0..400).map(|i| (i * 7) % 5).collect();
        for log in 4..=12u32 {
            let (table, encoded, bits) = tans_encode(&data, log);
            let decoded = tans_decode(&table, &encoded, bits, data.len());
            assert_eq!(decoded, data, "table_log = {log}");
        }
    }

    #[test]
    fn roundtrip_with_symbols_above_255() {
        let data: Vec<u32> = (0..900).map(|i| (i * 13) % 300).collect();
        let (table, encoded, bits) = tans_encode(&data, 12);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_with_a_table_larger_than_65536_entries() {
        let data: Vec<u32> = (0..900).map(|i| (i * 13) % 17).collect();
        let (table, encoded, bits) = tans_encode(&data, 20);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn compression_effectiveness() {
        let mut data = vec![0u32; 700];
        data.extend(std::iter::repeat_n(1u32, 200));
        data.extend(std::iter::repeat_n(2u32, 100));

        let (table, encoded, bits) = tans_encode(&data, 10);
        let decoded = tans_decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
        assert!(bits < data.len() * 8);
    }

    #[test]
    fn decode_table_covers_every_slot() {
        let freqs = vec![8u32, 8];
        let table = TansTable::new(&freqs, 4);

        let mut symbol_counts = vec![0u32; 2];
        for entry in &table.decode_table {
            symbol_counts[entry.symbol as usize] += 1;
        }
        assert_eq!(symbol_counts, vec![8, 8]);
    }

    #[test]
    fn decode_table_restores_a_state_inside_the_invariant() {
        // Every entry must rebuild a previous state in [L, 2L).
        let freqs = vec![10u32, 5, 1];
        let table = TansTable::new(&freqs, 4);
        for entry in &table.decode_table {
            let lowest = entry.new_state + table.table_size;
            let highest = lowest + (1u32 << entry.nb_bits) - 1;
            assert!(lowest >= table.table_size);
            assert!(
                highest < table.table_size * 2,
                "entry {entry:?} can leave the state range"
            );
        }
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

    #[test]
    fn absent_symbols_keep_their_slot_free() {
        let freqs = vec![12u32, 0, 4];
        let table = TansTable::new(&freqs, 4);
        assert_eq!(table.decode_table.len(), 16);
        assert!(table.decode_table.iter().all(|e| e.symbol != 1));
    }

    #[test]
    #[should_panic(expected = "frequency 0")]
    fn encoding_an_absent_symbol_fails_instead_of_hanging() {
        let table = TansTable::new(&[12, 0, 4], 4);
        let mut encoder = TansEncoder::new(&table);
        encoder.encode(1, &mut BitWriter::new());
    }
}
