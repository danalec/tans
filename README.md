# tANS/FSE: Entropy Coding Implementation

[![CI](https://github.com/danalec/tans/actions/workflows/ci.yml/badge.svg)](https://github.com/danalec/tans/actions/workflows/ci.yml)

A high-performance Rust implementation of **table Asymmetric Numeral Systems (tANS)** and **Finite State Entropy (FSE)** codecs, providing near-optimal compression with Huffman-like decoding speeds.

## Overview

This library implements entropy coders from the ANS (Asymmetric Numeral Systems) family, developed by Jarek Duda. ANS fills the gap between:

- **Huffman coding**: Fast but limited to integer bits per symbol
- **Arithmetic coding**: Optimal but computationally expensive

tANS/FSE achieves near-Shannon optimal compression while maintaining decode speeds comparable to Huffman coding, making it ideal for production compression systems.

### Key Features

- **Multiple ANS variants**: tANS (table-based), rANS (range-based), FSE (Collet's variant)
- **Zero dependencies**: Pure Rust implementation
- **Configurable table sizes**: Trade memory for precision
- **Comprehensive test suite**: Validates correctness across distributions
- **Benchmarking support**: Performance analysis with Criterion

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
tans = "0.1.0"
```

## Quick Start

```rust
use tans::{encode, decode};

// Encode data
let data = vec![0u32, 1, 0, 1, 0, 1, 0, 1];
let (table, encoded_bytes, bit_count) = encode(&data, 4); // log2(table_size) = 4

// Decode back
let decoded = decode(&table, &encoded_bytes, bit_count, data.len());
assert_eq!(decoded, data);
```

### Using Individual Components

```rust
use tans::{FseEncoder, FseDecoder, FseTable};

// Build table from frequencies
let frequencies = vec![700, 200, 100]; // Symbol 0: 70%, 1: 20%, 2: 10%
let table = FseTable::new(&frequencies, 10); // table size = 2^10

// Encode
let mut encoder = FseEncoder::new(&table);
for &symbol in &data {
    encoder.encode(symbol);
}
let (encoded, bits) = encoder.finish();

// Decode
let mut decoder = FseDecoder::new(&table, &encoded, bits);
let decoded: Vec<u32> = decoder.decode_all(data.len());
```

## API Reference

### High-Level Functions

| Function | Description |
|----------|-------------|
| `encode(data, log)` | Encode data with auto-generated frequency table |
| `decode(table, bytes, bits, n)` | Decode n symbols from bitstream |

### Core Types

| Type | Description |
|------|-------------|
| `Encoder` / `Decoder` | Basic rANS implementation |
| `TansEncoder` / `TansDecoder` | Full tANS with encoding table |
| `TableEncoder` / `TableDecoder` | Optimized table-based variant |
| `FseEncoder` / `FseDecoder` | FSE-style implementation |
| `RansTable` | Frequency and state tables |
| `BitWriter` / `BitReader` | Bitstream I/O utilities |

## Performance Characteristics

| Table Size (L) | Memory per table | Precision | Speed |
|----------------|------------------|-----------|-------|
| 2^8 = 256 | 1 KB | Poor | Very fast |
| 2^12 = 4096 | 16 KB | Good | Fast (L1 cache) |
| 2^16 = 65536 | 256 KB | Excellent | Medium |
| 2^20+ | >4 MB | Near-optimal | Slower (cache misses) |

`RansTable`/`StateTable` store one `u32` per slot; `FseTable`/`TansTable` store a
12-byte `DecodeEntry`. **Recommended**: L = 2^12 (4096 entries) for 256-symbol
alphabets — 16 KB keeps the hot table in L1 while providing good precision.

### Measured overhead over the Shannon entropy

Averaged over 12 orderings of the same multiset, so the number reflects the
codec rather than one lucky or unlucky arrangement:

| Input | Symbols | Entropy | Encoded (mean) | Overhead | Range over orderings |
|-------|---------|---------|----------------|----------|----------------------|
| Uniform, 256 symbols | 10 000 | 80 000 bits | 80 011 bits | +0.01% | identical for every ordering |
| 1/3 vs 2/3 | 10 000 | 9 184 bits | 10 011 bits | +9.01% | identical for every ordering |
| 70 / 20 / 10 | 1 000 | 1 157 bits | 1 180 bits | +2.04% | −4.65% … +10.91% |

Raising `table_log` does not change the gap — it is not state granularity. The
70/20/10 row has a wide spread because this state machine is not a complete
code (§8.6): some orderings of the same data encode *below* the entropy, others
well above it, and only the mean is meaningful. See §8.6.

## Benchmarks

Run benchmarks with:

```bash
cargo bench
```

---

# Technical Guide: tANS/FSE Theory and Implementation

This section provides a comprehensive explanation of the algorithms, theory, and implementation details.

## Table of Contents

1. [Introduction to ANS](#1-introduction-to-ans)
2. [Theoretical Foundations](#2-theoretical-foundations)
3. [Core Algorithms](#3-core-algorithms)
4. [Table Construction](#4-table-construction)
5. [Streaming & Normalization](#5-streaming--normalization)
6. [Implementation Details](#6-implementation-details)
7. [Common Bugs & Debugging](#7-common-bugs--debugging)
8. [Performance Optimization](#8-performance-optimization)
9. [Practical Applications](#9-practical-applications)
10. [References](#10-references)

---

## 1. Introduction to ANS

### 1.1 What is ANS?

ANS (Asymmetric Numeral Systems) is a family of entropy coding algorithms developed by Jarek Duda around 2009. Unlike traditional arithmetic coding which builds a floating-point number from the MSB (most significant bits), ANS encodes data by growing a single integer state from the LSB (least significant bits).

**Key Variants:**
- **rANS** (range ANS): Similar to range coding, uses arithmetic operations
- **tANS** (table ANS): Precomputed decoding tables, trade memory for speed
- **uABS** (unary/binary ANS): Binary alphabet version
- **FSE** (Finite State Entropy): Yann Collet's practical tANS implementation used in zstd

### 1.2 Historical Context

The ANS family emerged from independent work by:
- **Jarek Duda**: Original theoretical framework (arXiv:1311.2540)
- **Yann Collet**: Practical FSE implementation, influenced Duda's v2 paper
- **Fabian Giesen**: Interleaved encoding for SIMD, rANS optimizations
- **Charles Bloom**: Comprehensive analysis and implementation notes

ANS fills a sweet spot between Huffman coding (fast but limited to integer bits) and arithmetic coding (optimal but slower). For static probability distributions, ANS can be nearly as fast as Huffman while achieving near-Shannon limit compression.

### 1.3 Why Use tANS/FSE?

**Advantages over Huffman:**
- Encodes fractional bits (better compression for skewed distributions)
- No Huffman tree traversal overhead
- Table-driven decoding is cache-friendly

**Advantages over Arithmetic Coding:**
- 2-4x faster decoding
- Integer-only arithmetic (no precision management)
- Simpler implementation

**Production Users:**
- **zstd**: Uses FSE for entropy coding in LZ compression
- **Oodle**: RAD's commercial codec (PS5, Unreal Engine)
- **LZNA**: ANS-based codec for games

---

## 2. Theoretical Foundations

### 2.1 The Core Idea: State as Information Buffer

In ANS, the state variable `x` acts as a buffer accumulating fractional bits of information:

```
log₂(x) ≈ bits of information in state
```

Each encoding step grows the state by the entropy required for that symbol:

```
x' = x × (M/Fs) + Bs + (x mod Fs)
```

Where:
- `Fs` = frequency of symbol s
- `Bs` = cumulative frequency (CDF base)
- `M` = total frequency sum (Fs sum)
- `(x mod Fs)` preserves fractional bits critical for decoding

**Intuition**: The state grows proportionally to the inverse probability. High-probability symbols grow state slowly (few bits), low-probability symbols grow it quickly (many bits).

### 2.2 Comparison with Arithmetic Coding

| Aspect | Arithmetic Coding | ANS |
|--------|-------------------|-----|
| Direction | MSB → FIFO | LSB → LIFO |
| State variables | [low, range] | single x |
| Precision | Floating-point ranges | Integer state |
| Normalization | Check MSB stabilization | Fixed interval [L, bL] |
| Decoder speed | ~2-4x slower than Huffman | ~Huffman speed (tANS) |
| Memory | Minimal | Tables (tANS) or minimal (rANS) |

**Key difference**: Arithmetic coding builds the output number from most significant bits outward (MSB-first), while ANS builds from least significant bits (LSB-first). This means:
- Arithmetic coding: First symbol encoded = first bits of output
- ANS: First symbol encoded = last bits of output (must reverse)

### 2.3 Shannon Entropy Limit

The theoretical optimal code length for symbol s with probability p(s):

```
l(s) = -log₂(p(s))  bits
```

Average entropy: `H = -Σ p(s) × log₂(p(s))`

**Example**:
- Symbol with p = 0.5: optimal = 1 bit (Huffman achieves this)
- Symbol with p = 0.75: optimal = 0.415 bits (Huffman can only do 1 bit)
- Symbol with p = 0.9: optimal = 0.152 bits (Huffman can only do 1 bit)

Huffman can only achieve integer bit lengths (l(s) ∈ ℕ), but ANS encodes fractional bits by buffering state information across multiple symbols.

### 2.4 Information Preservation

The encoding function preserves information:

```
Information after = Information before + Information added
log₂(x') = log₂(x) + log₂(M/Fs)
```

The `(x mod Fs)` term is crucial — it preserves the "fractional bits" that allow perfect reconstruction during decoding.

---

## 3. Core Algorithms

### 3.1 rANS Encoding/Decoding

**Encoding Function:**
```cpp
// C(s, x) - encode symbol s from state x
uint32_t encode(uint32_t x, uint32_t s) {
    // x' = M × floor(x/Fs) + Bs + (x mod Fs)
    return M * (x / freq[s]) + base[s] + (x % freq[s]);
}
```

**Decoding Function:**
```cpp
// D(x) - decode to get symbol and previous state
struct DecodeResult { uint32_t symbol; uint32_t prev_state; };

DecodeResult decode(uint32_t x) {
    uint32_t slot = x % M;
    uint32_t s = find_symbol_from_slot(slot);  // Bs ≤ slot < Bs+Fs
    
    // prev_x = Fs × floor(x/M) + (slot - Bs)
    uint32_t prev_x = freq[s] * (x / M) + (slot - base[s]);
    
    return {s, prev_x};
}
```

**Key insight**: The `(x % Fs)` term in encoding preserves fractional bits that the decoder recovers via `(slot - Bs)`.

### 3.2 tANS Table-Based Decoding

The decoder behavior is fully determined by state, making it tableable:

```cpp
struct DecodeTableEntry {
    uint8_t symbol;        // decoded symbol
    uint8_t nbBits;        // bits to read for renormalization
    uint16_t newState;     // base state after normalization
};

// Decoding loop
DecodeTableEntry entry = decodeTable[state & (L - 1)];
output_symbol(entry.symbol);
int bits = read_bits(entry.nbBits);
state = entry.newState + bits;
```

**Why this works**: Each state maps to a unique symbol and knows exactly how many bits to read to reach the next valid state.

### 3.3 State Intervals

The fundamental constraint is maintaining state in a valid range:

```
I = [L, bL)  where L = table size, b = radix (usually 2)
```

For each symbol s, the precursor range must exist:

```
Is = {x : C(s,x) ∈ I} = [Fs, 2Fs - 1]
```

This **b-uniqueness** property guarantees encoder/decoder synchronization.

### 3.4 Detailed Encoding Walkthrough

Consider encoding the sequence [A, B, A] with:
- Frequencies: F_A = 3, F_B = 1, M = 4
- Initial state: x = 4 (must be in [L, 2L) where L = M = 4)

**Step 1: Encode A**
```
x' = 4 * (4/3) + 0 + (4 % 3) = 5 + 0 + 1 = 6
state = 6
```

**Step 2: Encode B**
```
x' = 4 * (6/1) + 3 + (6 % 1) = 24 + 3 + 0 = 27
state = 27
```

**Step 3: Encode A**
```
x' = 4 * (27/3) + 0 + (27 % 3) = 36 + 0 + 0 = 36
state = 36
```

Final state: 36 (encoded in bitstream as offset from L)

### 3.5 Detailed Decoding Walkthrough

Starting from state 36:

**Step 1: Decode**
```
slot = 36 % 4 = 0 → symbol A (slots 0,1,2 map to A, slot 3 maps to B)
prev_x = 3 * (36/4) + (0 - 0) = 27
state = 27
```

**Step 2: Decode**
```
slot = 27 % 4 = 3 → symbol B (slot 3)
prev_x = 1 * (27/4) + (3 - 3) = 6
state = 6
```

**Step 3: Decode**
```
slot = 6 % 4 = 2 → symbol A (slot 2)
prev_x = 3 * (6/4) + (2 - 0) = 4
state = 4
```

Decoded sequence: [A, B, A] (reversed from encoding order, or pre-reversed input)

---

## 4. Table Construction

### 4.1 Symbol Spread Algorithm

The order in which symbols are placed in the decoding table affects compression efficiency. FSE uses a deterministic "step" function:

```cpp
void build_decode_table(uint8_t* freqs, uint32_t L) {
    // Standard FSE step
    uint32_t step = (L >> 1) + (L >> 3) + 3;
    uint32_t mask = L - 1;
    
    uint8_t table[L];
    uint32_t pos = 0;
    
    for (uint32_t s = 0; s < alphabet_size; s++) {
        for (uint32_t i = 0; i < freqs[s]; i++) {
            table[pos] = s;
            pos = (pos + step) & mask;  // deterministic spread
        }
    }
    
    // Build decoding entries from spread
    build_entries_from_spread(table, L);
}
```

**Why spread matters**: Uniform distribution across slots minimizes "bunching" that causes coding inefficiency. The step formula ensures each symbol appears in diverse positions.

### 4.2 Normalization Thresholds

For each table position, compute how many bits to output:

```cpp
// For position i containing symbol s
uint32_t nbBits = (i < freq[s] * 2) ? 
    ceil_log2(freq[s]) : ceil_log2(freq[s]) + 1;

uint32_t threshold = freq[s] << nbBits;
```

Positions below threshold need n+1 bits; above threshold need n bits.

### 4.3 Table Example

For L=16, frequencies {7, 6, 3} (symbols A, B, C):

```
Position | Symbol | nbBits | newState | Notes
---------|--------|--------|----------|-------
    7    |   A    |   3    |    1     | below threshold
    8    |   A    |   4    |    0     | above threshold
    9    |   A    |   4    |    0     | above threshold
   ...    |  ...   |  ...   |   ...    |
```

The encoding table (packed C(s,x)) uses only M entries via:

```cpp
// Packed encoding table
packedTable[Bs + (x - Fs)] = newState;
```

### 4.4 Step Function Variants

Different step functions produce different spreads:

| Step Formula | Properties |
|--------------|------------|
| `L/2 + L/8 + 3` | FSE standard, good distribution |
| `L/2 + 1` | Simpler, slightly worse spread |
| `prime near L/2` | Alternative coprime approach |

The key requirement: step must be coprime with L (for power-of-2 L, step must be odd).

---

## 5. Streaming & Normalization

### 5.1 The Critical Asymmetry

**Decoder**: Reads bits *after* decoding a symbol (renormalize for next step)
**Encoder**: Emits bits *before* encoding a symbol (proactively shrink state)

This asymmetry is fundamental - reversing it causes immediate desynchronization.

### 5.2 Encoder Normalization Loop

```cpp
// CORRECT: normalize BEFORE encoding
while (x >= maxStateForSymbol[s]) {
    write_bits(x & ((1 << nbBits) - 1), nbBits);
    x >>= nbBits;
}
// Now apply coding step
x = encode_table[s].newState + (x - freq[s]);
```

### 5.3 Decoder Normalization Loop

```cpp
// CORRECT: decode THEN renormalize
DecodeEntry e = table[x & (L - 1)];
output(e.symbol);
x = e.newState | (x >> log2L);

// Renormalize to stay in [L, 2L-1]
while (x < L) {
    x = (x << 1) | read_bit();
}
```

### 5.4 b-Uniqueness Proof

For interval I = [L, bL):

**Uniqueness**: If x ∈ I, it's the only valid normalized state at that decoding stage.
- x ≥ L, so b×x + d ≥ b×L for any d ∈ [0, b)
- Cannot "skip over" I in a single normalization step

**Reachability**: If x < L, one normalization step always lands in I.
- x ≤ L-1, so b×x + d ≤ b×L - 1

This guarantees no ambiguity and encoder/decoder stay synchronized.

### 5.5 Complete Encoding Algorithm

```cpp
void encode_sequence(vector<uint8_t>& symbols) {
    uint32_t state = L;  // Initial state
    
    // Encode in reverse order (LIFO)
    for (int i = symbols.size() - 1; i >= 0; i--) {
        uint8_t s = symbols[i];
        
        // Normalize: ensure state is in valid precursor range
        while (state >= maxState[s]) {
            write_bit(state & 1);
            state >>= 1;
        }
        
        // Encode symbol
        state = encode_step(s, state);
    }
    
    // Flush final state
    write_bits(state - L, log2L + 1);
}
```

### 5.6 Complete Decoding Algorithm

```cpp
vector<uint8_t> decode_sequence(size_t count) {
    vector<uint8_t> output;
    
    // Read initial state
    uint32_t state = L + read_bits_from_end(log2L + 1);
    
    for (size_t i = 0; i < count; i++) {
        // Decode symbol
        DecodeEntry e = table[state - L];
        output.push_back(e.symbol);
        
        // Update state with normalization bits
        state = e.newState + read_bits(e.nbBits);
        
        // Renormalize
        while (state < L) {
            state = (state << 1) | read_bit();
        }
    }
    
    return output;  // Note: may need reversal depending on convention
}
```

---

## 6. Implementation Details

### 6.1 LIFO Structure

tANS is fundamentally a stack (Last In, First Out):
- Last symbol encoded = first symbol decoded
- Decoder reads bitstream in reverse order

**Encoder pattern**:
```cpp
// Encode backwards from end
for (int i = N-1; i >= 0; i--) {
    encode_symbol(stream[i]);
}
// Reverse bitstream, or decoder reads backwards
```

### 6.2 Flush and Initialization

The encoder's state always lives in `[L, 2L)`, which needs `log2L + 1` bits. Both
variants below are equivalent — pick one and use it on both sides:

```cpp
// Option A (what this crate does): write the full state, read it back as-is.
void flush(uint32_t state, BitWriter& writer) {
    writer.write(state, log2L + 1);
}
uint32_t init(BitReader& reader) {
    return reader.read(log2L + 1);   // already in [L, 2L)
}

// Option B: write the offset from L, read it back and add L.
void flush(uint32_t state, BitWriter& writer) {
    writer.write(state - L, log2L + 1);
}
uint32_t init(BitReader& reader) {
    return L + reader.read(log2L + 1);
}
```

**Common mistake**: pairing option A's flush with option B's init (or vice
versa) shifts every state by L. Because `state - L < L` fits in `log2L` bits,
both are safe as long as the reader adds back exactly what the writer removed.

### 6.3 Interleaved Multi-State ANS

For SIMD parallelism, use multiple interleaved states:

```cpp
// N interleaved states
uint32_t states[N];

// Encode: process in reverse order
for (int i = N-1; i >= 0; i--) {
    encode_with_state(states[i], symbol);
}

// Flush: write each state separately, in reverse
for (int i = N-1; i >= 0; i--) {
    writer.write(states[i] - L, log2L + 1);
}

// Decoder init: read in forward order
for (int i = 0; i < N; i++) {
    states[i] = L + reader.read_from_end(log2L + 1);
}
```

This allows N-way parallel decoding without stream multiplexing overhead.

### 6.4 Division Avoidance (rANS)

For encoder division by known frequency Fs, use reciprocal multiplication:

```cpp
// Precompute magic constants
struct EncSymbol {
    uint32_t freq;
    uint32_t bias;      // Bs
    uint32_t reciprocal; // for fast division
    uint32_t shift;
};

// Fast encode using multiplication instead of division
uint32_t fast_encode(uint32_t x, EncSymbol& sym) {
    uint32_t q = (x * sym.reciprocal) >> sym.shift;
    return M * q + sym.bias + (x - q * sym.freq);
}
```

See Alverson's "Integer Division Using Reciprocals" for magic constant derivation.

### 6.5 Bitstream Implementation

Bits are packed least-significant-first into a 64-bit accumulator and drained a
byte at a time; reading walks the same bits back in reverse write order.

```rust
pub struct BitWriter {
    acc: u64,      // bits not yet drained into `bytes`
    acc_bits: u32, // how many bits of `acc` are live (always < 8 after a write)
    bytes: Vec<u8>,
}

impl BitWriter {
    pub fn write(&mut self, value: u32, nbits: u32) {
        assert!(nbits <= 32);
        if nbits == 0 {
            return;
        }
        self.acc |= (value as u64) << self.acc_bits;
        self.acc_bits += nbits;

        while self.acc_bits >= 8 {
            self.bytes.push(self.acc as u8);
            self.acc >>= 8;
            self.acc_bits -= 8;
        }
    }

    pub fn finish(mut self) -> (Vec<u8>, usize) {
        let total_bits = self.bytes.len() * 8 + self.acc_bits as usize;
        if self.acc_bits > 0 {
            self.bytes.push(self.acc as u8);
        }
        (self.bytes, total_bits)
    }
}
```

`BitReader` reinterprets the buffer as little-endian `u64` words, so a read of up
to 32 bits never touches more than two words regardless of alignment. `acc_bits`
stays below 8 after every write, which is what keeps `value << acc_bits` inside 64
bits for a 32-bit value.

---

## 7. Common Bugs & Debugging

### 7.1 Diagnostic Checklist

| Symptom | Likely Cause |
|---------|--------------|
| First symbol correct, then garbage | Normalization direction reversed |
| Fixed byte offset shift | Flush writes wrong bit count |
| Correct for short inputs, wrong for long | Buffer boundary mishandled |
| All decodes wrong, seemingly random | Table spread mismatch |
| Every Nth symbol wrong (N=interleave) | Multi-state flush/init order wrong |
| Round-trip fails only past 256 symbols | Decode entry `symbol` field too narrow |
| Round-trip fails only at `log >= 17` | Decode entry `new_state` field too narrow |
| Suite hangs instead of failing | Encoding a symbol whose frequency is 0 |
| Decodes fine but ratio far from entropy | Check the ordering: the code is incomplete (§8.6) |

The last three are silent by construction: none of them produces an error, they
produce wrong bytes or no bytes at all. Type widths and the frequency-0 case are
covered by `tests/regression_tests.rs`.

### 7.2 Normalization Direction Bug

**Wrong (encoder normalizes AFTER coding):**
```cpp
// BROKEN
x = encode_step(s, x);
if (x >= maxState) {
    write_bits(...);
    x >>= nbBits;
}
```

**Correct (encoder normalizes BEFORE coding):**
```cpp
// CORRECT
while (x >= maxStateFor[s]) {
    write_bits(...);
    x >>= nbBits;
}
x = encode_step(s, x);
```

### 7.3 Table Construction Inconsistency

Encoder and decoder must use identical spread algorithm:

```cpp
// BROKEN: encoder uses different step
encoder_step = L / 2 + 3;
decoder_step = L / 2 + L / 8 + 3;  // mismatch!
```

Any deviation produces valid-looking but mismatched tables.

### 7.4 Flush/Init State Handling

```cpp
// WRONG: write full state including implicit leading 1
writer.write(state, log2L + 2);  // too many bits

// CORRECT: state is in [L, 2L-1], only write offset
writer.write(state - L, log2L + 1);
```

### 7.5 Debugging Strategy

**Test progression**:
1. **Single-symbol alphabet** (all same symbol): Eliminates state machine complexity
2. **Two-symbol 50/50**: Tests normalization only
3. **General distribution**: Tests full table construction

**Printf debugging**:
```cpp
// Track state evolution
printf("enc: s=%d, x=%d -> x'=%d, wrote %d bits\n", 
       s, x, x_new, nbBits);
printf("dec: x=%d -> s=%d, x'=%d, read %d bits\n",
       x, s, x_prev, nbBits);
```

### 7.6 Common Implementation Errors

```rust
// BUG: Off-by-one in frequency table
let cumul[s] = freqs[0..s].sum(); // Wrong: should be freqs[0..s]
let cumul[s] = freqs[0..=s].sum(); // Correct

// BUG: Wrong state initialization
let state = 0; // Wrong: must be in [L, 2L)
let state = L; // Correct

// BUG: Reading bits in wrong direction
reader.read_bits(n); // For encoder normalization
reader.read_bits_reverse(n); // For state initialization

// BUG: Not reversing symbol order
for s in symbols { encode(s); } // Wrong
for s in symbols.iter().rev() { encode(s); } // Correct
```

---

## 8. Performance Optimization

### 8.1 Table Size Trade-offs

| Table Size (L) | Memory | Precision | Speed |
|----------------|--------|-----------|-------|
| 2^8 = 256 | 256B | Poor | Very fast |
| 2^12 = 4096 | 4KB | Good | Fast (L1 fit) |
| 2^16 = 64K | 64KB | Excellent | Medium |
| 2^20+ | >1MB | Near-optimal | Slow (cache miss) |

Sweet spot: L=2^12 for 256-symbol alphabet (4KB fits L1 cache).

### 8.2 Alias Method for Large M

For large M without huge tables, use alias method:

```cpp
// O(1) symbol lookup with O(n) space
struct AliasEntry {
    uint32_t divider;
    uint32_t alias;
    uint32_t freq0, freq1;
};

// Decode
uint32_t slot = x % M;
uint32_t bucket = slot / K;  // K = M/N
uint32_t symbol = (slot < alias[bucket].divider) ?
    bucket : alias[bucket].alias;
```

This provides constant-time lookup with alphabet-sized tables.

### 8.3 SIMD Optimization

Interleaved encoding enables SIMD decoding:

```cpp
// 4-way interleaved decode
__m128i states = load_states();
for (int i = 0; i < symbols_per_iter; i++) {
    __m128i entries = gather_decode_table(states);
    // Parallel symbol extraction and renormalization
}
```

zstd and Oodle use 4-8 interleaved states for modern CPU optimization.

### 8.4 Cache-Aware Design

```cpp
// Pack decode entry tightly for cache efficiency
struct CompactEntry {
    uint8_t symbol;
    uint8_t nbBits;      // only 0-8 range needed
    uint16_t newState;   // packed
};  // 4 bytes per entry

// 4096 entries × 4 bytes = 16KB, fits comfortably in L1
```

### 8.5 Branch Prediction Optimization

```rust
// Predictable decode loop
loop {
    let entry = table[state as usize];
    output.push(entry.symbol);
    
    // Branchless renormalization
    let needs_bits = (state < L) as u32;
    state = (state << needs_bits) | read_bits(needs_bits);
    
    if output.len() >= count { break; }
}
```

### 8.6 The Code Is Not Complete: Compression Trades Variance, Not Average

The decoder reconstructs the previous state as:

```
x     = freq + (slot - cumul)        // in [freq, 2*freq)
nb    = log2L - floor(log2(x))
prev  = (x << nb) | read(nb)         // in [L, 2L)
```

`x` is fully determined by the slot, `nb` is fully determined by `x`, and the
bits the encoder emits are forced: it writes the low `nb` bits of the state it
already holds. The only choice is how many shifts to make, and the decoder
re-derives the same count, so the transition has no degrees of freedom.

That makes the code **incomplete**: for a fixed symbol the encoder maps `L`
possible states onto only `freq` possible new states, and the collapsed states
are told apart by the bits written rather than by the state. The total number of
bit patterns still balances, but individual inputs do not. Encoding 1000 symbols
of 70/20/10 data in twelve different orderings gives sizes from 1103 to 1283
bits — some orderings land 4.7% *below* the Shannon entropy of the input, which
no complete code can do, and others land 10.9% above it. The mean is +2.04%.

An earlier revision of this guide blamed long symbol runs for a flat ~10%
penalty. That is wrong: a round-robin ordering, which has no long runs at all,
produces the same 1283 bits as three solid runs. What matters is the arrangement
the state happens to be in, not the run lengths.

#### Interventions that were tried and did not help

| Change | 70/20/10 result |
|--------|-----------------|
| FSE state convention (decoder state in `[0, L)`, fixed `nb_bits`) | 1282 bits |
| FSE's step-function table spread | 1301 bits |
| Renormalize to the state that maximizes `log2(y/x)` instead of the minimum shift | 1282 bits |
| Search all 1024 legal initial states | 1283 bits (the current one already wins) |
| Interleaved states, N = 2 / 4 / 8 | 1294 / 1316 / 1356 bits |

None of them improves on the current transition, which is why the crate ships
one state machine rather than four. The uniform-alphabet result (+0.01%) shows
the transition itself is sound; what remains is the incompleteness above, and
closing it properly needs a *complete* code, which is a different design
(arithmetic coding, or a tANS variant with an encoding table and a per-symbol
encodable-state search) rather than a tweak to this one.

---

## 9. Practical Applications

### 9.1 Real-World Usage

- **zstd**: Uses FSE for entropy coding in LZ compression
- **Oodle**: RAD's commercial codec, licensed for PS5/Unreal Engine
- **LZNA**: ANS-based codec for games
- **Video codecs**: Potential for CABAC replacement

### 9.2 Integration with LZ

ANS works well with LZ family:

```cpp
// LZ + ANS pipeline
for (each token) {
    if (literal) {
        encode_literal(symbol);  // ANS encode
    } else {
        encode_match(length, offset);  // separate ANS streams
    }
}
```

Multiple ANS streams can be interleaved for parallelism.

### 9.3 Adaptive Coding

While ANS excels at static distributions, adaptation is possible:

1. **Table switching**: Precompute N distributions, switch contexts
2. **Periodic resampling**: Update frequencies every K symbols
3. **Alias rANS**: Supports cheap probability updates via divider shifts

Trade-off: Adaptation overhead vs compression gain.

### 9.4 Bits-Back ANS (BB-ANS)

**From arXiv:1901.04866**: ANS enables "bits-back" coding for latent variable models:

```python
# BB-ANS concept: encode latent variable, "get bits back" during decoding
def bits_back_encode(model, data):
    # Sample latent z from posterior
    z = model.sample_latent(data)
    
    # Encode z (costs bits)
    bits_used = encode(z)
    
    # Encode data given z (costs bits)
    bits_used += encode_data_given_z(data, z)
    
    # "Bits back" from decoding z's prior probability
    bits_back = -log2(prior(z))
    
    return bits_used - bits_back  # Net cost
```

**Applications**:
- Lossless image compression with VAEs (HiLLoC)
- Hierarchical latent models (Bit-Swap)
- State space models

### 9.5 ANS + Encryption

**From arXiv:1612.04662**: ANS tables can be perturbed for simultaneous compression + encryption:

```cpp
// Use PRNG with secret key to generate symbol spread
void build_encrypted_ans_table(uint8_t* freqs, uint32_t L, uint32_t key) {
    PRNG rng(key);
    uint32_t step = rng.next() % L;  // Randomized spread
    
    // Build table with secret spread function
    // Without key, decoder cannot reconstruct table
    for (each symbol placement) {
        pos = rng.next() % L;
        table[pos] = symbol;
    }
}
```

**Security properties**:
- Table permutation acts as substitution cipher
- No additional cost beyond table construction
- Suitable for IoT/embedded systems (lightweight)

---

## 10. References

### Primary Sources

1. **Jarek Duda**: "Asymmetric numeral systems" (arXiv:1311.2540)
   - https://arxiv.org/abs/1311.2540

2. **Fabian Giesen**: "rANS notes" (blog series)
   - https://fgiesen.wordpress.com/2014/02/02/rans-notes/
   - https://fgiesen.wordpress.com/2014/02/18/rans-with-static-probability-distributions/

3. **Charles Bloom**: "Understanding ANS" (blog series)
   - https://cbloomrants.blogspot.com/2014/02/02-18-14-understanding-ans-conclusion.html

4. **Yann Collet**: "FSE decoding" (blog series)
   - https://fastcompression.blogspot.com/2014/01/fse-decoding-how-it-works.html

5. **Akreson**: "Entropy coding by a beginner for beginners" (7-part series)
   - https://akreson.github.io/en/posts/entropy-encoding-part1/
   - Covers: Shannon entropy → AC → PPM → SEE → ANS

### Implementations

1. **ryg_rans**: Fabian Giesen's reference implementation
   - https://github.com/rygorous/ryg_rans

2. **FiniteStateEntropy**: Yann Collet's FSE library
   - https://github.com/Cyan4973/FiniteStateEntropy

3. **zstd**: Production ANS-based codec
   - https://github.com/facebook/zstd

### Technical Papers

1. **Interleaved ANS**: Giesen's SIMD/GPU approach
   - https://arxiv.org/abs/1402.3392

2. **Division by Reciprocals**: Alverson
   - http://citeseerx.ist.psu.edu/viewdoc/summary?doi=10.1.1.33.1710

3. **Alias Method**: Vose's algorithm
   - https://www.keithschwarz.com/darts-dice-coins/

---

## Appendix A: Key Formulas

### Encoding
```
C(s, x) = M × floor(x/Fs) + Bs + (x mod Fs)
```

### Decoding
```
D(x) = (s, Fs × floor(x/M) + (x mod M) - Bs)
where s satisfies: Bs ≤ (x mod M) < Bs + Fs
```

### b-Uniqueness Condition
```
I = [L, bL)  must satisfy L = kM for integer k
```

### State Interval per Symbol
```
Is = [Fs, 2Fs - 1]
```

### Shannon Entropy
```
H = -Σ p(s) × log₂(p(s))
```

### Average Code Length
```
L_avg = Σ p(s) × l(s)
```

### Deviation from Entropy
```
D = (L_avg - H) / H
```

---

## Appendix B: Code Templates

### Minimal tANS Decoder (Rust)

```rust
pub struct TansDecoder<'a> {
    table: &'a [DecodeEntry],
    state: u32,
    reader: BitReader<'a>,
    l: u32,
    log2l: u32,
}

#[derive(Clone, Copy)]
pub struct DecodeEntry {
    pub symbol: u32,
    pub nb_bits: u32,
    pub new_state: u32,
}

impl<'a> TansDecoder<'a> {
    pub fn decode(&mut self) -> u32 {
        let entry = &self.table[(self.state - self.l) as usize];
        let symbol = entry.symbol;
        
        // Get normalization bits and compute next state
        let bits = self.reader.read_bits(entry.nb_bits);
        self.state = entry.new_state + bits;
        
        // Renormalize if needed
        while self.state < self.l {
            self.state = (self.state << 1) | self.reader.read_bit();
        }
        
        symbol
    }
}
```

### Minimal tANS Encoder (Rust)

```rust
pub struct TansEncoder<'a> {
    table: &'a EncodeTable,
    state: u32,
    writer: BitWriter,
    l: u32,
}

impl<'a> TansEncoder<'a> {
    pub fn encode(&mut self, symbol: u32, freq: u32) {
        // Normalize: ensure state is in valid range
        let threshold = self.table.threshold(symbol);
        while self.state >= threshold {
            self.writer.write_bit(self.state & 1);
            self.state >>= 1;
        }
        
        // Encode: update state
        let entry = &self.table.entry(symbol, self.state - freq);
        self.state = entry.new_state;
    }
    
    pub fn finish(mut self) -> (Vec<u8>, u32) {
        // Write final state
        let final_bits = self.log2l + 1;
        self.writer.write(self.state - self.l, final_bits);
        self.writer.finish()
    }
}
```

---

## Appendix C: Theoretical Optimality

### C.1 Redundancy Bounds (Kosolobov 2022)

**Key result from arXiv:2201.02514**: tANS redundancy is bounded by:

```
Redundancy = Σ f_a × log(n/f_a) + O(σ + r)
```

Where:
- σ = alphabet size
- r = log₂(n) (table size in bits)
- The O(σ + r) term can be bounded by σ·log(e) + r

**Important finding**: This refutes Duda's conjecture that redundancy could be O(σ/n²) bits per symbol. The actual lower bound is Ω(σ + r), proven by counterexamples.

**Implication**: For large alphabets, tANS overhead is unavoidable but predictable. Use rANS when σ is large.

### C.2 Optimality Conditions

**From arXiv:2209.02228**:

1. **ANS is optimal** for sources with probability distribution described by natural powers of 1/2:
   - p(s) = 2^(-m) for integer m
   - These match perfectly with Huffman's constraint

2. **Markov chain analysis**: ANS state probabilities can be calculated precisely, enabling exact compression rate determination

3. **Algorithms for finding optimal ANS instances**:
   - **State probability approximation**: Choose symbol spread to minimize redundancy
   - **Probabilistic algorithm**: Find ANS instances arbitrarily close to optimal

---

## Appendix D: Comparison Summary

### D.1 ANS vs Other Entropy Coders

| Feature | Huffman | Arithmetic | tANS | rANS |
|---------|---------|------------|------|------|
| Speed (encode) | Very fast | Slow | Fast | Medium |
| Speed (decode) | Very fast | Slow | Very fast | Medium |
| Compression | Suboptimal | Optimal | Near-optimal | Near-optimal |
| Memory | Low | Low | Medium (tables) | Low |
| Adaptation | Expensive | Natural | Table switching | Alias method |
| Parallelism | Difficult | Difficult | Interleaved | SIMD-friendly |
| GPU support | No | No | Yes (dtANS) | Yes |

### D.2 Recommended Use Cases

| Scenario | Recommended |
|----------|-------------|
| Static distribution, small alphabet | tANS (FSE) |
| Static distribution, large alphabet | rANS |
| Adaptive, small alphabet | Alias rANS |
| Deep learning compression | BB-ANS |
| GPU acceleration | Interleaved rANS |
| IoT/embedded | ANS + encryption |
| LZ backend (zstd-style) | tANS + interleaving |

---

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.