// SPDX-License-Identifier: LicenseRef-tans-1.0
// Copyright (c) 2026 Dan Alec Yamaguchi
//
// Licensed under the tANS License in LICENSE: the MIT License and the Apache
// License 2.0 apply in full, and commercial use is restricted.

use tans::*;

#[test]
fn test_basic_encode_decode() {
    let data = vec![0, 1, 2, 0, 1, 2, 0, 1, 0, 0, 0, 1, 1, 2, 2];
    let (table, encoded, bits) = encode(&data, 4);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
}

#[test]
fn test_single_symbol() {
    let data = vec![0u32; 8];
    let (table, encoded, bits) = encode(&data, 4);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
}

#[test]
fn test_two_symbols_fifty_fifty() {
    let data = vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
    let (table, encoded, bits) = encode(&data, 4);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
}

#[test]
fn test_highly_skewed_distribution() {
    let mut data = vec![0u32; 90];
    data.extend(std::iter::repeat_n(1u32, 9));
    data.push(2);
    let (table, encoded, bits) = encode(&data, 8);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
}

#[test]
fn test_large_alphabet() {
    let mut data = Vec::new();
    for i in 0..256 {
        data.extend_from_slice(&[i, i, i]);
    }
    let (table, encoded, bits) = encode(&data, 12);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
}

#[test]
fn test_empty_input() {
    // The premise: a non-empty input encodes fine, so the failure below is
    // specifically about the empty input and not a broken table path.
    let ok = vec![0u32, 1, 0, 1];
    let (t, b, n) = encode(&ok, 4);
    assert_eq!(decode(&t, &b, n, ok.len()), ok);

    let data: Vec<u32> = vec![];
    let result = std::panic::catch_unwind(|| encode(&data, 4));
    assert!(result.is_err());
}

#[test]
fn test_compression_effectiveness() {
    let mut data = vec![0u32; 700];
    data.extend(std::iter::repeat_n(1u32, 200));
    data.extend(std::iter::repeat_n(2u32, 100));
    let (table, encoded, bits) = encode(&data, 10);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
    assert!(bits < data.len() * 8);
}

#[test]
fn test_different_table_sizes() {
    let data = vec![0, 1, 2, 0, 1, 2, 0, 1, 0, 0];
    for table_log in 4..=12u32 {
        let (table, encoded, bits) = encode(&data, table_log);
        let decoded = decode(&table, &encoded, bits, data.len());
        assert_eq!(decoded, data, "Failed for table_log = {}", table_log);
    }
}

#[test]
fn test_frequency_normalization() {
    let counts = vec![1000u64, 500, 250, 125, 62, 31, 16, 8];
    let normalized = normalize_frequencies(&counts, 256);
    let sum: u32 = normalized.iter().sum();
    assert_eq!(sum, 256);
}

#[test]
fn test_fse_table_properties() {
    let frequencies = vec![128, 64, 32, 16, 8, 4, 2, 2];
    let table = FseTable::new(&frequencies, 8);
    assert_eq!(table.table_size, 256);
    assert_eq!(table.frequencies, frequencies);
}

#[test]
fn test_bitstream() {
    let mut writer = BitWriter::new();
    writer.write(0b101, 3);
    writer.write(0b11001, 5);
    let (buffer, bits) = writer.finish();

    let mut reader = BitReader::from_bytes(&buffer, bits);
    assert_eq!(reader.read(5), 0b11001);
    assert_eq!(reader.read(3), 0b101);
}

#[test]
fn test_reverse_reader_order() {
    let mut writer = BitWriter::new();
    writer.write(0b10110, 5);
    writer.write(0b111, 3);
    writer.write(0b00, 2);

    let (buffer, bits) = writer.finish();
    let mut reader = BitReader::from_bytes(&buffer, bits);
    assert_eq!(reader.read(2), 0b00);
    assert_eq!(reader.read(3), 0b111);
    assert_eq!(reader.read(5), 0b10110);
}

#[test]
fn test_long_sequence() {
    let mut data = Vec::new();
    for i in 0..1000 {
        data.push((i % 100) as u32);
    }
    let (table, encoded, bits) = encode(&data, 12);
    let decoded = decode(&table, &encoded, bits, data.len());
    assert_eq!(decoded, data);
}
