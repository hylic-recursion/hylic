.PHONY: hylic-check hylic-test hylic-test-parallel hylic-test-all hylic-bench hylic-bench-report hylic-bench-full

# ── Quick checks ────────────────────────────────────────────
hylic-check:
	@cargo check --lib --tests --benches

# ── Tests ───────────────────────────────────────────────────
hylic-test:
	@cargo test --lib -- --nocapture

hylic-test-parallel:
	@cargo test --test test_eager --test test_eager_debug -- --nocapture

hylic-test-all: hylic-test hylic-test-parallel

# ── Benchmarks ──────────────────────────────────────────────
hylic-bench:
	@bash Makefile-scripting/bench-run.sh

hylic-bench-report:
	@python3 Makefile-scripting/bench-report.py

hylic-bench-full: hylic-bench hylic-bench-report
	@echo "Report: target/bench-report/bench-report.html"
