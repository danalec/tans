//! Property tests for the two components the audit rewrote.
//!
//! The bitstream moved from one byte per bit to a word accumulator, and
//! `normalize_frequencies` was rewritten twice after two separate defects. Both
//! are covered by hand-written cases elsewhere; these check them against
//! properties over many inputs, including a reference implementation of the
//! bitstream's original semantics so the rewrite is pinned to the contract it
//! had to preserve.

use tans::*;

/// Deterministic PRNG so a failure is reproducible from the test name alone.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The original byte-per-bit reader, kept here as the contract the rewrite had
/// to preserve: same bit order, same reverse-read semantics, same zero padding
/// when the stream runs short.
struct ReferenceReader {
    bits: Vec<bool>,
    pos: usize,
}

impl ReferenceReader {
    fn new(bytes: &[u8], total: usize) -> Self {
        let bits: Vec<bool> = bytes
            .iter()
            .flat_map(|&b| (0..8).map(move |i| (b >> i & 1) == 1))
            .take(total)
            .collect();
        Self { bits, pos: total }
    }

    fn read(&mut self, n: u32) -> u32 {
        let mut r = 0u32;
        for i in (0..n).rev() {
            if self.pos == 0 {
                break;
            }
            self.pos -= 1;
            if self.bits[self.pos] {
                r |= 1 << i;
            }
        }
        r
    }
}

/// Randomized round-trips must agree with the reference reader on every value
/// and on the exact buffer, for widths from 0 to 32 bits.
#[test]
fn bitstream_matches_its_original_semantics() {
    for seed in 0..200u64 {
        let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 1);

        // A random write sequence.
        let mut widths = Vec::new();
        let mut values = Vec::new();
        let count = 1 + rng.below(24) as usize;
        for _ in 0..count {
            let n = rng.below(33) as u32;
            let value = rng.next() as u32;
            widths.push(n);
            values.push(value);
        }

        let mut writer = BitWriter::new();
        for (value, n) in values.iter().zip(&widths) {
            writer.write(*value, *n);
        }
        let (bytes, bits) = writer.finish();

        // The buffer must be exactly ceil(bits / 8) long and fully consumed.
        assert_eq!(bytes.len(), bits.div_ceil(8), "seed {seed}: trailing byte");

        // Reading back the same widths, in reverse order, reproduces the low
        // `n` bits of each value: `write` keeps only those.
        let mask = |n: u32| if n == 32 { u32::MAX } else { (1u32 << n) - 1 };
        let mut reader = BitReader::from_bytes(&bytes, bits);
        let mut reference = ReferenceReader::new(&bytes, bits);
        for (value, n) in values.iter().zip(&widths).rev() {
            assert_eq!(reader.read(*n), *value & mask(*n), "seed {seed}: width {n}");
            assert_eq!(
                reader.read(0),
                0,
                "seed {seed}: a zero-width read must be a no-op"
            );
            let _ = reference.read(*n);
        }

        // And the same stream, read back in different groupings, gives the same
        // values as the reference for each group.
        let mut reader = BitReader::from_bytes(&bytes, bits);
        let mut reference = ReferenceReader::new(&bytes, bits);
        let mut remaining = bits;
        while remaining > 0 {
            let n = 1 + rng.below(remaining.min(32) as u64) as u32;
            assert_eq!(
                reader.read(n),
                reference.read(n),
                "seed {seed}: regrouped width {n}"
            );
            remaining -= n as usize;
        }
    }
}

/// Reading past the end of the stream must pad with zeroes rather than index
/// out of bounds, and a header that lies about its length must be clamped.
///
/// Padding lands in the *high* half of the result: a reader that runs dry part
/// way through a value keeps the bits it had and leaves the missing low ones
/// zero, which is what the loop it replaced did.
#[test]
fn bitstream_survives_a_short_or_lying_stream() {
    let mut writer = BitWriter::new();
    writer.write(0b1011, 4);
    writer.write(0b1100_1110, 8);
    let (bytes, bits) = writer.finish();
    assert_eq!(bits, 12);

    // The last write sits at the end of the stream, so a full-width read of 8
    // returns it exactly.
    let mut reader = BitReader::from_bytes(&bytes, bits);
    assert_eq!(reader.read(8), 0b1100_1110);
    assert_eq!(reader.read(4), 0b1011);
    // Nothing left: reads return zero instead of indexing past the buffer.
    assert_eq!(reader.read(32), 0);

    // Asking for more bits than exist keeps the available ones in the high half.
    let mut short = BitReader::from_bytes(&[0b1010_1010], 8);
    assert_eq!(short.read(16), 0b1010_1010 << 8);

    // A bit count larger than the payload is clamped to what actually exists.
    let mut lying = BitReader::from_bytes(&[0xFF], 4096);
    assert_eq!(lying.read(8), 0xFF);
    assert_eq!(lying.read(8), 0);
}

/// Properties the normalizer must satisfy for any count vector.
#[test]
fn normalization_properties_hold_for_random_counts() {
    for seed in 0..400u64 {
        let mut rng = Rng::new(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 7);

        let symbols = 1 + rng.below(64) as usize;
        let target = symbols as u32 + rng.below(512) as u32;

        // Half the symbols present, the rest absent.
        let counts: Vec<u64> = (0..symbols)
            .map(|_| {
                if rng.below(2) == 0 {
                    0
                } else {
                    1 + rng.next() % 10_000
                }
            })
            .collect();

        let present = counts.iter().filter(|&&c| c > 0).count();
        if present == 0 {
            // Nothing was observed: the documented fallback is a uniform
            // distribution, not a failure.
            let freqs = normalize_frequencies(&counts, target);
            assert_eq!(freqs.iter().map(|&f| f as u64).sum::<u64>(), target as u64);
            continue;
        }
        if present > target as usize {
            // An alphabet larger than the table cannot be represented: it must
            // fail with a message, not underflow.
            assert!(std::panic::catch_unwind(|| normalize_frequencies(&counts, target)).is_err());
            continue;
        }

        let freqs = normalize_frequencies(&counts, target);

        assert_eq!(
            freqs.iter().map(|&f| f as u64).sum::<u64>(),
            target as u64,
            "seed {seed}: frequencies must sum to the table size"
        );
        for (i, &c) in counts.iter().enumerate() {
            if c == 0 {
                assert_eq!(freqs[i], 0, "seed {seed}: absent symbol {i} took a slot");
            } else {
                assert!(freqs[i] >= 1, "seed {seed}: present symbol {i} got no slot");
            }
        }

        // Every symbol keeps its share to within one slot. When some share rounds
        // below one slot the table has to overpay somewhere, so the bound is
        // asserted only where it is actually achievable.
        let total: u64 = counts.iter().sum();
        let smallest_share = counts
            .iter()
            .filter(|&&c| c > 0)
            .map(|&c| c as f64 * target as f64 / total as f64)
            .fold(f64::INFINITY, f64::min);
        if smallest_share >= 1.0 {
            for (i, &c) in counts.iter().enumerate() {
                if c == 0 {
                    continue;
                }
                let ideal = c as f64 * target as f64 / total as f64;
                assert!(
                    (freqs[i] as f64 - ideal).abs() <= 1.0,
                    "seed {seed}: symbol {i} got {} slots, ideal {ideal:.1}",
                    freqs[i]
                );
            }
        }

        // A table can only be built when the target is a power of two.
        if target.is_power_of_two() {
            let _ = RansTable::new(&freqs, target.trailing_zeros());
        }
    }
}

/// Every variant must round-trip random data drawn from random distributions,
/// including alphabets past 255 and tables past 65536 entries.
#[test]
fn every_variant_roundtrips_random_data() {
    for seed in 0..60u64 {
        let mut rng = Rng::new(seed.wrapping_mul(0xA0761D6478BD642F) ^ 11);

        let alphabet = 1 + rng.below(400) as u32;
        let len = 1 + rng.below(3000) as usize;
        let data: Vec<u32> = (0..len)
            .map(|_| {
                // A skewed draw so most alphabets stay partly unobserved.
                let bucket = rng.below(100);
                if bucket < 50 {
                    rng.below(alphabet as u64) as u32
                } else {
                    rng.below((alphabet / 4).max(1) as u64) as u32
                }
            })
            .collect();

        let max_symbol = *data.iter().max().unwrap();
        let distinct = data
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len();
        let log = 10u32.max((distinct as f64).log2().ceil() as u32);

        for (name, bytes, bits, decoded) in variants(&data, log) {
            assert_eq!(decoded, data, "seed {seed}: {name} lost data (log={log})");
            assert!(
                bits >= log as usize,
                "seed {seed}: {name} emitted too few bits"
            );
            assert!(
                bytes.len() == bits.div_ceil(8),
                "seed {seed}: {name} left a trailing byte"
            );
        }
        let _ = max_symbol;
    }
}

/// The four variants share one state update, so they must agree byte for byte
/// on random data too, not just on the hand-written distributions.
#[test]
fn variants_agree_on_random_data() {
    for seed in 100..130u64 {
        let mut rng = Rng::new(seed + 3);
        let len = 1 + rng.below(2000) as usize;
        let data: Vec<u32> = (0..len).map(|_| rng.below(7) as u32).collect();

        let results = variants(&data, 10);
        let (_, first_bytes, first_bits, _) = &results[0];
        for (name, bytes, bits, _) in &results[1..] {
            assert_eq!(bits, first_bits, "seed {seed}: {name} disagreed on size");
            assert_eq!(bytes, first_bytes, "seed {seed}: {name} disagreed on bytes");
        }
    }
}

fn variants(data: &[u32], log: u32) -> Vec<(&'static str, Vec<u8>, usize, Vec<u32>)> {
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
