#[derive(Debug, Clone)]
pub struct RansTable {
    pub log2_table_size: u32,
    pub table_size: u32,
    pub frequencies: Vec<u32>,
    pub cumul: Vec<u32>,
    pub decode_table: Vec<u32>,
}

impl RansTable {
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

        let mut cumul = Vec::with_capacity(freqs.len() + 1);
        cumul.push(0);
        for &f in freqs {
            cumul.push(cumul.last().unwrap() + f);
        }

        let mut decode_table = vec![0u32; table_size as usize];
        for (sym, &freq) in freqs.iter().enumerate() {
            for slot in cumul[sym]..cumul[sym] + freq {
                decode_table[slot as usize] = sym as u32;
            }
        }

        Self {
            log2_table_size: table_log,
            table_size,
            frequencies: freqs.to_vec(),
            cumul,
            decode_table,
        }
    }

    pub fn find_symbol(&self, slot: u32) -> u32 {
        assert!(
            slot < self.table_size,
            "slot {slot} is outside the table (size {})",
            self.table_size
        );
        self.decode_table[slot as usize]
    }
}

/// Largest table exponent the crate supports; keeps `freq << 1` inside `u32`.
pub(crate) const MAX_TABLE_LOG: u32 = 30;

/// Scales `counts` so the frequencies sum to exactly `target`.
///
/// Symbols that never occur get frequency 0 and therefore claim no table slot;
/// symbols that do occur always get at least one slot. The arithmetic is
/// integer-only so the result does not depend on floating-point rounding.
pub fn normalize_frequencies(counts: &[u64], target: u32) -> Vec<u32> {
    if counts.is_empty() {
        return vec![];
    }

    let present: Vec<usize> = (0..counts.len()).filter(|&i| counts[i] > 0).collect();
    assert!(
        present.len() <= target as usize,
        "alphabet has {} distinct symbols but the table only has {target} slots; raise table_log",
        present.len()
    );

    if present.is_empty() {
        // No symbol was observed: fall back to a uniform distribution so the
        // table still satisfies the "frequencies sum to the table size" contract.
        return uniform_frequencies(counts.len(), target);
    }

    let total: u128 = counts.iter().map(|&c| c as u128).sum();
    let target_u = target as u128;

    let mut freqs = vec![0u32; counts.len()];
    let mut sum: u128 = 0;
    let mut remainders: Vec<(usize, u128)> = Vec::with_capacity(present.len());
    for &i in &present {
        // Start from the floor of each exact share, raised to one slot so that
        // every symbol that occurred is representable. Rounding first and
        // fixing up afterwards could hand a symbol a second slot it did not
        // earn, pushing it more than one slot off its share.
        let exact = counts[i] as u128 * target_u;
        let floor = (exact / total).max(1);
        freqs[i] = floor as u32;
        sum += floor;
        remainders.push((i, exact % total));
    }

    // Hand the leftover slots to the symbols that lost the most fraction. With
    // floors as the base this guarantees every symbol lands within one slot of
    // its exact share, rather than up to one and a half.
    remainders.sort_by_key(|&(_, remainder)| std::cmp::Reverse(remainder));
    let diff = target_u as i64 - sum as i64;

    if diff > 0 {
        let steps = (diff as usize).min(remainders.len());
        for (i, _) in remainders.iter().take(steps) {
            freqs[*i] += 1;
        }
    } else if diff < 0 {
        // Only reachable when raising the smallest shares to one slot overflows
        // the table, which needs a symbol holding more than one slot to give one
        // up -- a symbol that occurred must never end up with none.
        let donors: Vec<usize> = remainders
            .iter()
            .filter(|(i, _)| freqs[*i] > 1)
            .map(|(i, _)| *i)
            .collect();
        let steps = ((-diff) as usize).min(donors.len());
        for i in donors.iter().take(steps) {
            freqs[*i] -= 1;
        }
    }

    debug_assert_eq!(freqs.iter().map(|&f| f as u64).sum::<u64>(), target as u64);
    freqs
}

fn uniform_frequencies(symbols: usize, target: u32) -> Vec<u32> {
    assert!(
        symbols <= target as usize,
        "alphabet has {symbols} symbols but the table only has {target} slots; raise table_log"
    );
    let base = target / symbols as u32;
    let mut extra = target % symbols as u32;
    (0..symbols)
        .map(|_| {
            let f = base + u32::from(extra > 0);
            extra = extra.saturating_sub(1);
            f
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let t = RansTable::new(&[8, 8], 4);
        assert_eq!(t.table_size, 16);
    }

    #[test]
    fn normalization_keeps_the_sum_and_zero_counts() {
        let counts = vec![500u64, 0, 0, 0, 0, 500];
        let freqs = normalize_frequencies(&counts, 16);
        assert_eq!(freqs.iter().sum::<u32>(), 16);
        assert_eq!(freqs, vec![8, 0, 0, 0, 0, 8]);
    }

    #[test]
    fn normalization_tracks_the_distribution() {
        let counts = vec![700u64, 200, 100];
        let freqs = normalize_frequencies(&counts, 1024);
        assert_eq!(freqs.iter().sum::<u32>(), 1024);
        // 0.7 / 0.2 / 0.1 of 1024, with no slot wasted on absent symbols.
        assert_eq!(freqs, vec![717, 205, 102]);
    }

    #[test]
    fn normalization_survives_a_lopsided_alphabet() {
        // 300 distinct symbols cannot each hold a slot in a 256-entry table.
        let counts: Vec<u64> = (0..300).map(|i| 300 - i).collect();
        let result = std::panic::catch_unwind(|| normalize_frequencies(&counts, 256));
        assert!(
            result.is_err(),
            "a table smaller than the alphabet must fail"
        );
    }

    #[test]
    fn normalization_of_an_empty_alphabet_is_empty() {
        assert!(normalize_frequencies(&[], 16).is_empty());
    }

    #[test]
    fn normalization_without_counts_stays_uniform_and_sums() {
        let freqs = normalize_frequencies(&[0, 0, 0, 0], 10);
        assert_eq!(freqs.iter().sum::<u32>(), 10);
        assert_eq!(freqs, vec![3, 3, 2, 2]);
    }

    #[test]
    fn find_symbol_maps_every_slot() {
        let t = RansTable::new(&[3, 0, 5], 3);
        assert_eq!(t.find_symbol(0), 0);
        assert_eq!(t.find_symbol(2), 0);
        assert_eq!(t.find_symbol(3), 2);
        assert_eq!(t.find_symbol(7), 2);
    }

    #[test]
    #[should_panic(expected = "outside the table")]
    fn find_symbol_rejects_an_out_of_range_slot() {
        RansTable::new(&[8, 8], 4).find_symbol(16);
    }
}
