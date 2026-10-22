// SPDX-License-Identifier: LicenseRef-tans-1.0
// Copyright (c) 2026 Dan Alec Yamaguchi
//
// Licensed under the tANS License in LICENSE: the MIT License and the Apache
// License 2.0 apply in full, and commercial use is restricted.

use crate::bitstream::BitWriter;
use crate::rans_table::{RansTable, MAX_TABLE_LOG};

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
        let freq = *table.frequencies.get(sym as usize).unwrap_or_else(|| {
            panic!(
                "symbol {sym} is outside the table alphabet of {}",
                table.frequencies.len()
            )
        });
        assert!(
            freq > 0,
            "symbol {sym} has frequency 0 and cannot be encoded"
        );

        // Renormalize down to [freq, 2*freq) so the next step cannot overflow.
        let x_max = freq << 1;
        while self.state >= x_max {
            writer.write(self.state & 1, 1);
            self.state >>= 1;
        }

        let start = table.cumul[sym as usize];
        let q = self.state / freq;
        let r = self.state % freq;
        self.state = (q << table.log2_table_size) + r + start;
    }

    pub fn finish(&self, table: &RansTable, writer: &mut BitWriter) {
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

/// Twelve bits of table precision: the crate's documented sweet spot for
/// byte-sized alphabets.
impl Default for Config {
    fn default() -> Self {
        Self { log: 12 }
    }
}

pub fn encode_with_stats(data: &[u32], cfg: Config) -> (RansTable, Vec<u8>, usize) {
    assert!(!data.is_empty(), "cannot build a table from an empty input");
    assert!(
        cfg.log <= MAX_TABLE_LOG,
        "table_log must be <= {MAX_TABLE_LOG} (got {})",
        cfg.log
    );

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
