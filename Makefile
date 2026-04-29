# ── Minibuild local CI ──────────────────────────────────────────
# Every target mirrors a GitHub Actions job. Run `make ci` before
# committing to catch the same errors the pipeline would.
#
#   make ci        — full suite (what CI runs)
#   make quick     — fast subset (fmt + clippy + test)
#   make fmt       — auto-format code
#   make fix       — auto-fix clippy + fmt
#
# Prerequisites: rustup components rustfmt, clippy (installed by default).
# Optional: cargo-deny (`cargo install cargo-deny`).

CARGO        := cargo
MSRV         := 1.78.0
RUSTFLAGS_CI := -Dwarnings

.PHONY: ci quick fmt fmt-check clippy test msrv docs lockfile deny fix clean help

# ── Aggregate targets ─────────────────────────────────────────────

## Run the full CI suite locally (mirrors every GitHub Actions job)
ci: fmt-check clippy test docs lockfile deny
	@echo ""
	@echo "All CI checks passed."

## Fast pre-commit check (formatting + lint + tests)
quick: fmt-check clippy test
	@echo ""
	@echo "Quick checks passed."

# ── Individual CI jobs ────────────────────────────────────────────

## Check formatting (mirrors CI: Rustfmt)
fmt-check:
	$(CARGO) fmt --all --check

## Run clippy with warnings-as-errors (mirrors CI: Clippy)
clippy:
	RUSTFLAGS="$(RUSTFLAGS_CI)" $(CARGO) clippy --all-targets -- -D warnings

## Run test suite (mirrors CI: Test)
test:
	$(CARGO) test --locked

## Check MSRV compilation (mirrors CI: MSRV 1.78.0) — requires rustup
msrv:
	rustup run $(MSRV) $(CARGO) check --locked

## Build documentation (mirrors CI: Documentation)
docs:
	RUSTDOCFLAGS="$(RUSTFLAGS_CI)" $(CARGO) doc --no-deps --locked

## Verify Cargo.lock is up-to-date (mirrors CI: lockfile)
lockfile:
	$(CARGO) check --locked

## Run cargo-deny checks (mirrors CI: Security Audit — requires cargo-deny)
deny:
	@command -v cargo-deny >/dev/null 2>&1 && cargo deny check || echo "cargo-deny not installed, skipping (install: cargo install cargo-deny)"

# ── Developer helpers ─────────────────────────────────────────────

## Auto-format code
fmt:
	$(CARGO) fmt --all

## Auto-fix clippy warnings + format
fix:
	$(CARGO) fmt --all
	$(CARGO) clippy --all-targets --fix --allow-dirty --allow-staged

## Remove build artifacts and cache
clean:
	$(CARGO) clean
	rm -f .minibuild_cache

## Show available targets
help:
	@echo "Targets:"
	@echo "  make ci        Full CI suite (what GitHub Actions runs)"
	@echo "  make quick     Fast pre-commit (fmt + clippy + test)"
	@echo "  make fmt       Auto-format code"
	@echo "  make fix       Auto-fix clippy + format"
	@echo "  make test      Run tests"
	@echo "  make clippy    Run clippy"
	@echo "  make docs      Build documentation"
	@echo "  make deny      Run cargo-deny (security/license checks)"
	@echo "  make msrv      Check MSRV (1.78.0) compilation"
	@echo "  make clean     Remove build artifacts"