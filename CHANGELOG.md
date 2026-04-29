# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-06-28

### Added

- Declarative Buildfile DSL with named rules, dependencies, inputs/outputs, environment variables, and phony targets
- DAG-based dependency resolver with cycle detection and topological sorting
- Parallel execution engine with configurable worker pool (`--jobs N`)
- Real-time stdout/stderr streaming with rule-name attribution
- Fail-fast execution with downstream job cancellation
- Timestamp-based incremental build cache (`.minibuild_cache`)
- CLI with `--file`, `--jobs`, `--clean`, `--dry-run`, `--verbose` flags
- `$VAR` and `${VAR}` variable expansion in commands
- 35 tests covering diamond deps, 100+ rule graphs, incremental rebuilds, and failure propagation