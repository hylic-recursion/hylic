.PHONY: hylic-check hylic-test hylic-test-parallel hylic-test-all \
       hylic-bench hylic-bench-modes hylic-bench-overhead hylic-bench-module \
       hylic-bench-report hylic-bench-full hylic-docs-build hylic-docs-serve

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
	@bash Makefile-scripting/bench-run.sh all

hylic-bench-modes:
	@bash Makefile-scripting/bench-run.sh modes

hylic-bench-overhead:
	@bash Makefile-scripting/bench-run.sh overhead

hylic-bench-module:
	@bash Makefile-scripting/bench-run.sh module

hylic-bench-report:
	@python3 Makefile-scripting/bench-report.py

hylic-bench-full: hylic-bench hylic-bench-report hylic-docs-build
	@echo "Done: benchmarks + report + docs rebuilt"

# ── Docs ────────────────────────────────────────────────────
hylic-docs-build:
	@cd ../hylic-docs/book && mdbook build

hylic-docs-serve:
	@fuser -k 8321/tcp 2>/dev/null || true
	@cd ../hylic-docs/book && mdbook serve -p 8321
