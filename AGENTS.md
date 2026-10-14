# AGENTS.md

tANS/FSE entropy codecs (rANS, table tANS, FSE) as a zero-dependency Rust library — one module per codec
variant, sharing bitstream I/O and frequency normalization.

## Setup commands

- Install deps: `cargo build` (no runtime dependencies; only `criterion` for benches)
- Build:        `cargo build --release` (release profile already sets `opt-level = 3`, `lto = true`)
- Test:         `cargo test`
- Bench:        `cargo bench` (criterion, `harness = false`, single `tans_bench` target)
- Lint:         `cargo clippy --all-targets`
- Format:       `cargo fmt`
- Fast gate:    `cargo check --all-targets`

## Project layout

- `src/lib.rs` — module wiring, the public `encode`/`decode` convenience API (rANS), and crate-level unit tests
- `src/encoder.rs`, `src/decoder.rs` — rANS state machine; what `encode`/`decode` actually call
- `src/table_ans.rs` — table tANS: `StateTable` spread plus `TableEncoder`/`TableDecoder`
- `src/tans_full.rs` — full tANS with explicit encoding table (`TansTable`, `EncodeEntry`)
- `src/fse.rs` — FSE (Collet) variant: `FseTable`, `FseEncoder`, `FseDecoder`, `DecodeEntry`
- `src/rans_table.rs` — `RansTable` and `normalize_frequencies`, shared by every variant
- `src/bitstream.rs` — `BitWriter`/`BitReader`; LSB-first, decoded in reverse write order
- `tests/integration_tests.rs` — cross-module round-trip and table-property tests
- `benches/tans_bench.rs` — criterion benches per variant plus round-trip
- `README.md` — API docs and a long technical guide (theory, table construction, common bugs); keep its code examples in sync with `src/`

## Code style

- rustfmt defaults; there is no `rustfmt.toml` and no `[lints]` table — do not introduce formatting churn
- Zero `unsafe`, integer-only arithmetic; README §7 holds the debug checklist when a round-trip desyncs
- Public types are re-exported from `src/lib.rs` — a new public type needs a matching `pub use`
- Validate preconditions with `assert!`/`assert_eq!` (e.g. frequencies must sum to the table size), never silently repair them
- Doc comments are currently absent; match the surrounding style unless asked otherwise

## Testing instructions

- Unit tests: `cargo test` — `#[cfg(test)] mod tests` in `src/lib.rs`; integration tests in `tests/integration_tests.rs`
- Every variant must round-trip: `decode(encode(data)) == data` for uniform, skewed, and single-symbol inputs
- Compression assertions are expected, not optional (`assert!(bits < data.len() * 8)`)
- Tests that assert a failure mode must make their own premise explicit (see `test_empty_input`) — a test that never verifies its precondition can pass for the wrong reason
- Run heavy suites and benchmarks on the real Linux host over SSH (`danalec@192.168.0.40`), not WSL; keep Windows runs to compile gates

## PR & commit conventions

- Branch from `master`; never push to it directly
- Conventional commits (`feat:` / `fix:` / `docs:` / `test:` / `perf:`), subject under 72 chars
- No CI is configured — run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` before opening a PR

## Security

- Never commit secrets; `.gitignore` already covers `target/`, `Cargo.lock`, and editor/IDE files
- Dual licensed `MIT OR Apache-2.0` — leave `LICENSE-MIT` and `LICENSE-APACHE` intact, and do not relicense unilaterally
- No `unsafe`, no network, no filesystem I/O; the dependency-free codec is a stated project property, keep it that way

## License

`MIT OR Apache-2.0`