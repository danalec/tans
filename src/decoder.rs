// SPDX-License-Identifier: LicenseRef-tans-1.0
// Copyright (c) 2026 Dan Alec Yamaguchi
//
// Licensed under the tANS License in LICENSE: the MIT License and the Apache
// License 2.0 apply in full, and commercial use is restricted.

use crate::bitstream::BitReader;
use crate::rans_table::RansTable;

pub struct Decoder<'a> {
    table: &'a RansTable,
    state: u32,
}

impl<'a> Decoder<'a> {
    pub fn new(table: &'a RansTable, reader: &mut BitReader) -> Self {
        let init = reader.read(table.log2_table_size + 1);
        Self { table, state: init }
    }

    pub fn decode(&mut self, reader: &mut BitReader) -> u32 {
        let table_size = self.table.table_size;
        let slot = self.state & (table_size - 1);
        let sym = self.table.find_symbol(slot);

        let freq = self.table.frequencies[sym as usize];
        let start = self.table.cumul[sym as usize];

        self.state = freq * (self.state >> self.table.log2_table_size) + slot - start;

        // Pull the bits the encoder shifted out in one read instead of one bit at
        // a time: shifting `needed` places is exactly what brings the state back
        // into [L, 2L).
        if self.state < table_size {
            assert!(self.state > 0, "decoded a zero state from a corrupt stream");
            let needed = self.table.log2_table_size - (31 - self.state.leading_zeros());
            self.state = (self.state << needed) | reader.read(needed);
        }

        sym
    }
}

pub fn decode_all(table: &RansTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    let mut r = BitReader::from_bytes(bytes, bits);
    let mut dec = Decoder::new(table, &mut r);
    (0..n).map(|_| dec.decode(&mut r)).collect()
}
