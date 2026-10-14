//! Regression tests for defects found while auditing the crate.
//!
//! Each test here corresponds to a bug that shipped and was reproduced before
//! being fixed; the comments record what actually went wrong, because the shape
//! of the failure is easy to forget once it is gone.

use tans::*;

/// rANS / table tANS / full tANS / FSE, all driven through their one-shot API.
fn all_variants(data: &[u32], log: u32) -> Vec<(&'static str, Vec<u8>, usize, Vec<u32>)> {
    let n = data.len();

    let (rt, rb, rn) = encode(data, log);
    let rans = decode(&rt, &rb, rn, n);

    let (tt, tb, tn) = table_encode(data, log);
    let table = table_decode(&tt, &tb, tn, n);

    let (at, ab, an) = tans_encode(data, log);
    let tans = tans_decode(&at, &ab, an, n);

    let (ft, fb, fn_) = fse_encode(data, log);
    let fse = fse_decode(&ft, &fb, fn_, n);

    vec![
        ("rans", rb, rn, rans),
        ("table", tb, tn, table),
        ("tans", ab, an, tans),
        ("fse", fb, fn_, fse),
    ]
}

fn distributions() -> Vec<(&'static str, Vec<u32>)> {
    vec![
        ("uniform-2", vec![0, 1, 0, 1, 0, 1, 0, 1]),
        ("uniform-4", (0..200).map(|i| i % 4).collect()),
        ("uniform-256", (0..2000).map(|i| i % 256).collect()),
        ("single", vec![7u32; 64]),
        (
            "skewed-70-20-10",
            (0..1000)
                .map(|i| {
                    if i < 700 {
                        0
                    } else if i < 900 {
                        1
                    } else {
                        2
                    }
                })
                .collect(),
        ),
        (
            "sparse-alphabet",
            (0..400).map(|i| if i % 3 == 0 { 0 } else { 200 }).collect(),
        ),
        ("one-run-then-other", {
            let mut v = vec![0u32; 200];
            v.extend(std::iter::repeat_n(1u32, 200));
            v
        }),
    ]
}

#[test]
fn every_variant_roundtrips_every_distribution() {
    for (name, data) in distributions() {
        for log in 6..=12u32 {
            if (1u32 << log) < data.iter().copied().max().unwrap_or(0) + 1 {
                continue; // table too small for the alphabet
            }
            for (variant, _, bits, decoded) in all_variants(&data, log) {
                assert_eq!(decoded, data, "{variant} lost data on {name} at log={log}");
                assert!(bits > 0, "{variant} produced an empty stream for {name}");
            }
        }
    }
}

/// The four variants share one renormalisation loop, so they must agree on the
/// encoded size. A mismatch means one of them grew a different state rule.
#[test]
fn variants_agree_on_the_encoded_size() {
    for (name, data) in distributions() {
        for log in 6..=12u32 {
            if (1u32 << log) < data.iter().copied().max().unwrap_or(0) + 1 {
                continue;
            }
            let results = all_variants(&data, log);
            let (_, first_bytes, first_bits, _) = &results[0];
            for (variant, bytes, bits, _) in &results[1..] {
                assert_eq!(
                    bits, first_bits,
                    "{variant} produced {bits} bits where rans produced {first_bits} on {name} at log={log}"
                );
                assert_eq!(
                    bytes, first_bytes,
                    "{variant} produced a different byte stream than rans on {name} at log={log}"
                );
            }
        }
    }
}

/// The old normalizer forced every alphabet entry to at least one slot, so a
/// sparse alphabet with a small table drove the rounding loop into an
/// underflow panic (`attempt to subtract with overflow`).
#[test]
fn sparse_alphabet_with_a_small_table_roundtrips() {
    let data: Vec<u32> = (0..64).map(|i| if i % 2 == 0 { 0 } else { 255 }).collect();
    for log in 8..=12u32 {
        for (variant, _, _, decoded) in all_variants(&data, log) {
            assert_eq!(decoded, data, "{variant} failed on a sparse alphabet");
        }
    }
}

/// `DecodeEntry.symbol` was a `u8`, so any symbol above 255 was silently
/// truncated and the round-trip produced the wrong data without an error.
#[test]
fn variants_handle_symbols_above_255() {
    let data: Vec<u32> = (0..900).map(|i| (i * 13) % 300).collect();
    for (variant, _, _, decoded) in all_variants(&data, 12) {
        assert_eq!(decoded, data, "{variant} truncated a symbol above 255");
    }
}

/// `DecodeEntry.new_state` was a `u16`, which overflows once the table has more
/// than 65536 entries. tANS ignored the field and survived; FSE used it and
/// corrupted silently.
#[test]
fn variants_handle_tables_above_65536_entries() {
    let data: Vec<u32> = (0..900).map(|i| (i * 13) % 17).collect();
    for log in [16u32, 17, 20] {
        for (variant, _, _, decoded) in all_variants(&data, log) {
            assert_eq!(decoded, data, "{variant} corrupted at log={log}");
        }
    }
}

/// A symbol with frequency 0 made every encoder's renormalisation loop
/// `while state >= 2 * 0`, which never terminates.
#[test]
#[should_panic(expected = "frequency 0")]
fn encoding_an_absent_symbol_panics_instead_of_hanging() {
    let table = RansTable::new(&[8, 0, 8], 4);
    Encoder::new(&table).encode(&table, 1, &mut BitWriter::new());
}

#[test]
fn absent_symbols_round_trip_in_every_variant() {
    // Symbols that never occur must not consume table slots.
    let data: Vec<u32> = (0..300).map(|i| if i % 4 == 0 { 3 } else { 9 }).collect();
    for (variant, _, _, decoded) in all_variants(&data, 10) {
        assert_eq!(decoded, data, "{variant} failed with an absent symbol");
    }
}

#[test]
fn normalization_never_exceeds_the_table_capacity() {
    // More distinct symbols than slots cannot be represented: it must fail with
    // a message, not underflow.
    let counts: Vec<u64> = (1..=300).collect();
    let result = std::panic::catch_unwind(|| normalize_frequencies(&counts, 256));
    assert!(result.is_err());

    let freqs = normalize_frequencies(&[1, 1, 1, 1], 256);
    assert_eq!(freqs, vec![64, 64, 64, 64]);
}

/// A table log past the supported maximum must fail with a message naming the
/// limit, not by shifting `1u32 << log` past the width or allocating gigabytes.
#[test]
#[should_panic(expected = "table_log must be <= 30")]
fn rans_rejects_an_oversized_table_log() {
    let _ = encode(&[0u32, 1, 0, 1], 31);
}

#[test]
#[should_panic(expected = "table_log must be <= 30")]
fn table_tans_rejects_an_oversized_table_log() {
    let _ = StateTable::new(&[1 << 31, 1 << 31], 31);
}

#[test]
#[should_panic(expected = "table_log must be <= 30")]
fn full_tans_rejects_an_oversized_table_log() {
    let _ = TansTable::new(&[1 << 31, 1 << 31], 31);
}

#[test]
#[should_panic(expected = "table_log must be <= 30")]
fn fse_rejects_an_oversized_table_log() {
    let _ = FseTable::new(&[1 << 31, 1 << 31], 31);
}

#[test]
fn normalization_handles_degenerate_shapes() {
    // One symbol taking the whole table.
    assert_eq!(normalize_frequencies(&[9], 8), vec![8]);
    // One present symbol among many absent ones keeps every slot.
    assert_eq!(normalize_frequencies(&[5, 0, 0, 0], 8), vec![8, 0, 0, 0]);
    // A table with exactly one slot holds exactly one symbol.
    assert_eq!(normalize_frequencies(&[10, 0], 1), vec![1, 0]);
    assert!(std::panic::catch_unwind(|| normalize_frequencies(&[4, 6], 1)).is_err());
    // Empty in, empty out.
    assert!(normalize_frequencies(&[], 16).is_empty());
}

#[test]
#[should_panic(expected = "only has 0 slots")]
fn normalization_rejects_an_empty_table() {
    let _ = normalize_frequencies(&[1, 1], 0);
}

/// Pins the compression the crate currently achieves, so a change to the
/// renormalization loop cannot quietly make it much worse.
///
/// The bound is a worst case: this state machine is incomplete, so the same
/// multiset encodes to different sizes depending on its ordering (README 8.6).
/// For 70/20/10 that spread is 1103..1283 bits against an entropy of 1157, so
/// the single-shot figure is the top of the range, not the typical one.
#[test]
fn compression_overhead_stays_bounded() {
    let cases: Vec<(&str, Vec<u32>, f64, u32)> = vec![
        (
            "uniform-256",
            (0..10000).map(|i| (i % 256) as u32).collect(),
            8.0,
            10,
        ),
        (
            "skewed-70-20-10",
            (0..1000)
                .map(|i| {
                    if i < 700 {
                        0
                    } else if i < 900 {
                        1
                    } else {
                        2
                    }
                })
                .collect(),
            1.156_748, // entropy of 0.7 / 0.2 / 0.1
            10,
        ),
    ];

    for (name, data, bits_per_symbol, log) in cases {
        let results = all_variants(&data, log);
        let (_, _, bits, _) = &results[0];
        let ideal = bits_per_symbol * data.len() as f64;
        assert!(
            *bits as f64 <= ideal * 1.15,
            "{name}: {bits} bits against an ideal of {ideal:.0} (log={log})"
        );
    }
}

/// The encoded size depends on the ordering of the same multiset, because the
/// state machine is not a complete code. This pins that the *mean* over
/// orderings stays near the entropy: a bound on one ordering could be met by
/// luck, but a mean that drifts up means every ordering got worse.
#[test]
fn mean_over_orderings_stays_near_the_entropy() {
    let runs: Vec<u32> = (0..1000)
        .map(|i| {
            if i < 700 {
                0
            } else if i < 900 {
                1
            } else {
                2
            }
        })
        .collect();

    // A stride coprime with the length visits every position exactly once, so
    // each ordering holds exactly the same multiset.
    let n = runs.len();
    let mut sizes = Vec::new();
    for stride in [1usize, 7, 101, 211, 307, 401, 499, 601, 809, 907, 337] {
        if gcd(stride, n) != 1 {
            continue;
        }
        let mut data = Vec::with_capacity(n);
        let mut i = 0usize;
        while data.len() < n {
            data.push(runs[i]);
            i = (i + stride) % n;
        }
        assert_eq!(data.iter().filter(|&&s| s == 0).count(), 700);

        let (_, _, bits, decoded) = &all_variants(&data, 10)[0];
        assert_eq!(decoded, &data, "stride {stride} changed the data");
        sizes.push(*bits);
    }

    let entropy = 1156.8f64;
    let mean = sizes.iter().map(|&b| b as f64).sum::<f64>() / sizes.len() as f64;
    assert!(
        mean <= entropy * 1.05,
        "mean of {sizes:?} is {mean:.1} bits against an entropy of {entropy:.0}"
    );
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

#[test]
fn a_stream_never_grows_the_input() {
    // Sanity floor for the encoder: even the worst case tested stays below the
    // raw 4-byte-per-symbol representation.
    for (name, data) in distributions() {
        let results = all_variants(&data, 10);
        let (_, _, bits, decoded) = &results[0];
        assert_eq!(decoded, &data, "{name}");
        assert!(
            *bits <= data.len() * 32 + 64,
            "{name} emitted {bits} bits for {} symbols",
            data.len()
        );
    }
}
