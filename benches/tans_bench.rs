use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tans::{decode, encode, encode_all, BitWriter, Encoder, RansTable};

fn bench_encode_small(c: &mut Criterion) {
    let data: Vec<u32> = (0..100).map(|i| i % 4).collect();
    let frequencies = vec![64, 64, 64, 64];
    let table = RansTable::new(&frequencies, 8);

    c.bench_function("encode_small_100", |b| {
        b.iter(|| {
            let mut encoder = Encoder::new(&table);
            let mut writer = BitWriter::new();
            for &sym in data.iter().rev() {
                encoder.encode(&table, sym, &mut writer);
            }
            encoder.finish(&table, &mut writer);
            let (bytes, bits) = writer.finish();
            black_box((bytes, bits))
        })
    });
}

fn bench_decode_small(c: &mut Criterion) {
    let data: Vec<u32> = (0..100).map(|i| i % 4).collect();
    let (table, encoded, bits) = encode(&data, 8);

    c.bench_function("decode_small_100", |b| {
        b.iter(|| decode(&table, &encoded, bits, data.len()))
    });
}

fn bench_encode_medium(c: &mut Criterion) {
    let data: Vec<u32> = (0..10000).map(|i| i % 256).collect();
    let frequencies: Vec<u32> = (0..256).map(|_| 16).collect();
    let table = RansTable::new(&frequencies, 12);

    c.bench_function("encode_medium_10k", |b| {
        b.iter(|| {
            let mut encoder = Encoder::new(&table);
            let mut writer = BitWriter::new();
            for &sym in data.iter().rev() {
                encoder.encode(&table, sym, &mut writer);
            }
            encoder.finish(&table, &mut writer);
            let (bytes, bits) = writer.finish();
            black_box((bytes, bits))
        })
    });
}

fn bench_decode_medium(c: &mut Criterion) {
    let data: Vec<u32> = (0..10000).map(|i| i % 256).collect();
    let (table, encoded, bits) = encode(&data, 12);

    c.bench_function("decode_medium_10k", |b| {
        b.iter(|| decode(&table, &encoded, bits, data.len()))
    });
}

fn bench_table_construction(c: &mut Criterion) {
    let frequencies: Vec<u32> = vec![128, 64, 32, 16, 8, 4, 2, 2];

    c.bench_function("table_construction_small", |b| {
        b.iter(|| RansTable::new(&frequencies, 8))
    });
}

fn bench_table_construction_large(c: &mut Criterion) {
    let mut frequencies: Vec<u32> = (0..256)
        .map(|i| {
            if i < 64 {
                64
            } else if i < 128 {
                32
            } else if i < 192 {
                16
            } else {
                8
            }
        })
        .collect();
    let sum: u32 = frequencies.iter().sum();
    let target = 4096u32;
    let diff = target as i32 - sum as i32;
    if diff != 0 {
        frequencies[0] = (frequencies[0] as i32 + diff) as u32;
    }

    c.bench_function("table_construction_large", |b| {
        b.iter(|| RansTable::new(&frequencies, 12))
    });
}

fn bench_roundtrip(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();

    c.bench_function("roundtrip_1k", |b| {
        b.iter(|| {
            let (table, encoded, bits) = encode(&data, 10);
            let decoded = decode(&table, &encoded, bits, data.len());
            black_box(decoded)
        })
    });
}

fn bench_encode_all_api(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let frequencies: Vec<u32> = vec![64; 16];
    let table = RansTable::new(&frequencies, 10);

    c.bench_function("encode_all_1k", |b| b.iter(|| encode_all(&table, &data)));
}

fn bench_table_encode(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let (table, _, _) = tans::table_encode(&data, 10);

    c.bench_function("table_encode_1k", |b| {
        b.iter(|| tans::table_encode_all(&table, &data))
    });
}

fn bench_table_decode(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let (table, encoded, bits) = tans::table_encode(&data, 10);

    c.bench_function("table_decode_1k", |b| {
        b.iter(|| tans::table_decode(&table, &encoded, bits, data.len()))
    });
}

fn bench_tans_encode(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let (table, _, _) = tans::tans_encode(&data, 10);

    c.bench_function("tans_encode_1k", |b| {
        b.iter(|| tans::tans_encode_all(&table, &data))
    });
}

fn bench_tans_decode(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let (table, encoded, bits) = tans::tans_encode(&data, 10);

    c.bench_function("tans_decode_1k", |b| {
        b.iter(|| tans::tans_decode(&table, &encoded, bits, data.len()))
    });
}

fn bench_fse_encode(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let (table, _, _) = tans::fse_encode(&data, 10);

    c.bench_function("fse_encode_1k", |b| {
        b.iter(|| tans::fse_encode_all(&table, &data))
    });
}

fn bench_fse_decode(c: &mut Criterion) {
    let data: Vec<u32> = (0..1000).map(|i| i % 16).collect();
    let (table, encoded, bits) = tans::fse_encode(&data, 10);

    c.bench_function("fse_decode_1k", |b| {
        b.iter(|| tans::fse_decode(&table, &encoded, bits, data.len()))
    });
}

fn bench_bitstream_write(c: &mut Criterion) {
    let mut values = BitWriter::new();
    for i in 0..1024u32 {
        values.write(i.wrapping_mul(2_654_435_761), 12);
    }
    let (bytes, bits) = values.finish();

    c.bench_function("bitstream_write_12k", |b| {
        b.iter(|| {
            let mut w = BitWriter::new();
            for i in 0..1024u32 {
                w.write(i.wrapping_mul(2_654_435_761), 12);
            }
            w.finish()
        })
    });

    c.bench_function("bitstream_read_12k", |b| {
        b.iter(|| {
            let mut r = tans::BitReader::from_bytes(&bytes, bits);
            let mut acc = 0u32;
            for _ in 0..1024 {
                acc = acc.wrapping_add(r.read(12));
            }
            black_box(acc)
        })
    });
}

criterion_group!(
    benches,
    bench_encode_small,
    bench_decode_small,
    bench_encode_medium,
    bench_decode_medium,
    bench_table_construction,
    bench_table_construction_large,
    bench_roundtrip,
    bench_encode_all_api,
    bench_table_encode,
    bench_table_decode,
    bench_tans_encode,
    bench_tans_decode,
    bench_fse_encode,
    bench_fse_decode,
    bench_bitstream_write,
);

criterion_main!(benches);
