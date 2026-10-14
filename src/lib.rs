pub mod bitstream;
pub mod decoder;
pub mod encoder;
pub mod fse;
pub mod rans_table;
pub mod table_ans;
pub mod tans_full;

pub use bitstream::{BitReader, BitWriter};
pub use decoder::{decode_all, Decoder};
pub use encoder::{encode_all, encode_with_stats, Config, Encoder};
pub use fse::{
    fse_decode, fse_decode_all, fse_encode, fse_encode_all, DecodeEntry, FseDecoder, FseEncoder,
    FseTable,
};
pub use rans_table::{normalize_frequencies, RansTable};
pub use table_ans::{
    table_decode, table_decode_all, table_encode, table_encode_all, StateTable, TableDecoder,
    TableEncoder,
};
pub use tans_full::{
    tans_decode, tans_decode_all, tans_encode, tans_encode_all, TansDecoder, TansEncoder, TansTable,
};

pub fn encode(data: &[u32], log: u32) -> (RansTable, Vec<u8>, usize) {
    encode_with_stats(data, Config { log })
}

pub fn decode(table: &RansTable, bytes: &[u8], bits: usize, n: usize) -> Vec<u32> {
    decode_all(table, bytes, bits, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_uniform() {
        let data = vec![0u32, 1, 0, 1, 0, 1, 0, 1];
        let (table, encoded, bits) = encode(&data, 4);

        println!("encoded bits: {}", bits);
        println!("encoded bytes: {:?}", encoded);
        println!("frequencies: {:?}", table.frequencies);
        println!("cumul: {:?}", table.cumul);

        for slot in 0..16 {
            let sym = table.find_symbol(slot);
            println!("slot {} -> symbol {}", slot, sym);
        }

        let decoded = decode(&table, &encoded, bits, data.len());
        println!("decoded: {:?}", decoded);
        println!("expected: {:?}", data);

        assert_eq!(decoded, data);
    }

    #[test]
    fn uniform() {
        let data = vec![0u32, 1, 0, 1, 0, 1, 0, 1];
        let (table, encoded, bits) = encode(&data, 4);
        let decoded = decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn single() {
        let d = vec![0u32; 8];
        let (t, b, n) = encode(&d, 4);
        assert_eq!(decode(&t, &b, n, d.len()), d);
    }

    #[test]
    fn skewed() {
        let d = vec![0u32, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2];
        let (t, b, n) = encode(&d, 4);
        println!(
            "skewed: frequencies={:?}, cumul={:?}, bits={}, bytes={:?}",
            t.frequencies, t.cumul, n, b
        );
        let decoded = decode(&t, &b, n, d.len());
        println!("skewed: decoded={:?}, expected={:?}", decoded, d);
        assert_eq!(decoded, d);
    }

    #[test]
    fn larger() {
        let d: Vec<u32> = (0..50).map(|i| i % 4).collect();
        let (t, b, n) = encode(&d, 6);
        assert_eq!(decode(&t, &b, n, d.len()), d);
    }

    #[test]
    fn compress() {
        let mut d = vec![0u32; 700];
        d.extend(std::iter::repeat_n(1u32, 200));
        d.extend(std::iter::repeat_n(2u32, 100));
        let (t, b, n) = encode(&d, 10);
        assert_eq!(decode(&t, &b, n, d.len()), d);
        assert!(n < d.len() * 8);
    }
}
