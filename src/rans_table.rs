#[derive(Debug, Clone)]
pub struct RansTable {
    pub log2_table_size: u32,
    pub table_size: u32,
    pub frequencies: Vec<u32>,
    pub cumul: Vec<u32>,
}

impl RansTable {
    pub fn new(freqs: &[u32], table_log: u32) -> Self {
        let table_size = 1u32 << table_log;
        assert_eq!(freqs.iter().sum::<u32>(), table_size);

        let mut cumul = Vec::with_capacity(freqs.len() + 1);
        cumul.push(0);
        for &f in freqs {
            cumul.push(cumul.last().unwrap() + f);
        }

        Self {
            log2_table_size: table_log,
            table_size,
            frequencies: freqs.to_vec(),
            cumul,
        }
    }

    pub fn find_symbol(&self, slot: u32) -> u32 {
        for (sym, &f) in self.frequencies.iter().enumerate() {
            if f > 0 {
                let start = self.cumul[sym];
                if slot >= start && slot < start + f {
                    return sym as u32;
                }
            }
        }
        0
    }
}

pub fn normalize_frequencies(counts: &[u64], target: u32) -> Vec<u32> {
    if counts.is_empty() {
        return vec![];
    }
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return vec![1; counts.len()];
    }

    let mut freqs: Vec<u32> = counts
        .iter()
        .map(|&c| ((c as f64 / total as f64 * target as f64).round() as u32).max(1))
        .collect();

    let mut sum: i64 = freqs.iter().map(|&f| f as i64).sum();
    while sum != target as i64 {
        if sum > target as i64 {
            let idx = freqs
                .iter()
                .enumerate()
                .filter(|(_, &f)| f > 1)
                .max_by_key(|(_, &f)| f)
                .map(|(i, _)| i)
                .unwrap_or(0);
            freqs[idx] -= 1;
            sum -= 1;
        } else {
            let idx = counts
                .iter()
                .enumerate()
                .max_by_key(|(_, c)| **c)
                .map(|(i, _)| i)
                .unwrap_or(0);
            freqs[idx] += 1;
            sum += 1;
        }
    }
    freqs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let t = RansTable::new(&[8, 8], 4);
        assert_eq!(t.table_size, 16);
    }
}
