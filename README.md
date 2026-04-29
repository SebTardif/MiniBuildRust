# Minibuild

A parallel build system written in Rust with **zero external dependencies**. Minibuild reads a declarative build file, resolves dependencies into a DAG, and executes independent jobs concurrently across a configurable worker pool — with incremental rebuild support so unchanged targets are skipped automatically.

Think of it as a simplified, from-scratch alternative to Make or Ninja.

## Features

- **Declarative Buildfile DSL** — named rules with shell commands, explicit dependencies, input/output file declarations, environment variables, and phony targets
- **Dependency resolution** — constructs a DAG, detects circular dependencies with clear error messages, and topologically sorts for correct execution order
- **Parallel execution** — runs independent jobs concurrently using a thread-per-job worker pool with configurable parallelism (`--jobs N`)
- **Real-time output** — streams stdout/stderr with `[rule_name]` attribution so interleaved output stays readable
- **Fail-fast** — when a rule fails, all downstream dependents are immediately cancelled
- **Incremental builds** — timestamp-based change detection skips rules whose inputs and outputs haven't changed since the last successful run
- **Zero dependencies** — built entirely on Rust's standard library

## Quick Start

```bash
# Build
cargo build --release

# Run with the included example Buildfile
cargo run

# Or use the binary directly
./target/release/minibuild
```

## CLI Usage

```
minibuild [OPTIONS] [TARGET]

Options:
  --file, -f <FILE>   Build file path (default: Buildfile)
  --jobs, -j <N>      Max parallel jobs (default: number of CPU cores)
  --clean             Remove the build cache and rebuild everything
  --dry-run, -n       Print what would be executed without running anything
  --verbose, -v       Show detailed execution info
  --help, -h          Show help
```

### Examples

```bash
# Build the default target
minibuild

# Build a specific target with 4 parallel jobs
minibuild --jobs 4 compile

# See what would run without executing
minibuild --dry-run

# Clean cached state and rebuild from scratch
minibuild --clean all

# Use a different build file
minibuild --file build.mb deploy
```

## Buildfile Format

Buildfiles use a simple indentation-based DSL:

```
# Global environment variables
env CC = gcc
env CFLAGS = -Wall -O2

# Default target when none is specified on the command line
default all

rule all
  deps compile link
  description Build the project
  phony true
  run echo "Build complete"

rule compile
  inputs src/main.c src/util.c
  outputs build/main.o build/util.o
  env EXTRA = -DDEBUG
  run mkdir -p build
  run $CC $CFLAGS $EXTRA -c src/main.c -o build/main.o
  run $CC $CFLAGS $EXTRA -c src/util.c -o build/util.o

rule link
  deps compile
  inputs build/main.o build/util.o
  outputs build/app
  run $CC build/main.o build/util.o -o build/app
```

### Directives

**Top-level:**

| Directive | Description |
|---|---|
| `env KEY = VALUE` | Set a global environment variable |
| `default <target>` | Default target when none given on CLI |
| `rule <name>` | Begin a rule block |

**Inside a rule block** (indented):

| Directive | Description |
|---|---|
| `deps <rule> [rule...]` | Rules that must complete before this one |
| `inputs <file> [file...]` | Input files (used for incremental build detection) |
| `outputs <file> [file...]` | Output files (used for incremental build detection) |
| `env KEY = VALUE` | Rule-scoped environment variable (overrides globals) |
| `run <command>` | Shell command to execute (multiple allowed, run in order) |
| `description <text>` | Human-readable description of the rule |
| `phony true` | Mark as phony — always re-execute, never cache |

### Variable Expansion

Commands support `$VAR` and `${VAR}` expansion from both global and rule-level environment variables:

```
env PREFIX = /usr/local

rule install
  run cp build/app $PREFIX/bin/
  run echo "Installed to ${PREFIX}/bin/"
```

## Incremental Builds

Minibuild tracks the modification timestamps of each rule's `inputs` and `outputs` files. On subsequent runs, rules are skipped if:

1. The rule has `inputs` declared
2. All input and output timestamps match the last successful build
3. The rule is not marked `phony`

State is stored in `.minibuild_cache` in the working directory. Use `--clean` to reset it.

## Architecture

```
src/
  main.rs       Entry point — wires CLI, parser, graph, executor, and cache together
  cli.rs        Argument parser (hand-rolled, no external crates)
  parser.rs     Buildfile DSL parser with variable expansion
  graph.rs      DAG construction, cycle detection (3-color DFS), topological sort (Kahn's)
  executor.rs   Parallel scheduler with thread worker pool and mpsc coordination
  cache.rs      Timestamp-based incremental build cache with serialization
```

## Tests

The project includes 35 tests covering:

- **Diamond dependencies** — A depends on B and C, both depend on D
- **Large graphs** — 120-rule chains and 110-leaf fan-out graphs to stress the scheduler
- **Incremental rebuilds** — verify skip on no-change, rebuild on input modification
- **Failure propagation** — deep chain failure cancels all downstream rules
- **Partial failure** — independent branches succeed even when a sibling fails
- **Phony rules** — always re-execute regardless of cache
- **Cycle detection** — 2-node and 3-node cycles with clear error paths
- **Environment variable expansion** — `$VAR`, `${VAR}`, and missing variable handling
- **CLI parsing** — flags, short forms, edge cases

```bash
cargo test
```

## License

MIT