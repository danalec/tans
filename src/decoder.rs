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
        let slot = self.state & (self.table.table_size - 1);
        let sym = self.table.find_symbol(slot);

        let freq = self.table.frequencies[sym as usize];
        let start = self.table.cumul[sym as usize];

        self.state = freq * (self.state >> self.table.log2_table_size) + slot - start;

        while self.state < self.table.table_size {
            let bit = reader.read(1);
            self.state = (self.state << 1) | bit;
        }

        sym
    }
}

pub fn decode_all(table: &RansTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    let mut r = BitReader::from_bytes(bytes, bits);
    let mut dec = Decoder::new(table, &mut r);
    (0..n).map(|_| dec.decode(&mut r)).collect()
}
