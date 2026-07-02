# Minibuild

## Project overview

A parallel, incremental build system written in Rust with zero external
dependencies. Reads a declarative Buildfile DSL, resolves dependencies
into a DAG, and executes independent jobs concurrently.

## Build and test

```bash
make ci           # Full CI suite (fmt, clippy, test, docs, lockfile, deny)
make quick        # Fast pre-commit: fmt + clippy + test
make test         # Run tests only
```

MSRV: 1.78.0. Toolchain: stable (see `rust-toolchain.toml`).

## Project structure

```
src/
  main.rs       Entry point
  cli.rs        Argument parser (hand-rolled, no external crates)
  parser.rs     Buildfile DSL parser with variable expansion
  graph.rs      DAG construction, cycle detection, topological sort
  executor.rs   Parallel scheduler with thread worker pool
  cache.rs      Timestamp-based incremental build cache
```

## Conventions

- Zero external dependencies. Everything uses `std` only.
- Tests live in each module as `#[cfg(test)] mod tests`.
- Integration tests are in `src/main.rs` under `mod integration_tests`.
- Clippy warnings are errors in CI (`-D warnings`).
- Format with `cargo fmt --all` before committing.
