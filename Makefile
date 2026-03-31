.PHONY: check test bench bench-report bench-full test-eager test-all

# ── Quick checks ────────────────────────────────────────────
check:
	@cargo check --lib --tests --benches

# ── Tests ───────────────────────────────────────────────────
test:
	@cargo test --lib -- --nocapture

test-eager:
	@cargo test --test test_eager --test test_eager_debug -- --nocapture

test-all: test test-eager

# ── Benchmarks ──────────────────────────────────────────────
bench:
	@bash Makefile-scripting/bench-run.sh

bench-report:
	@python3 Makefile-scripting/bench-report.py

bench-full: bench bench-report
	@echo "Report: target/bench-report/bench-report.html"
