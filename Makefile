.PHONY: hylic-check hylic-test hylic-test-parallel hylic-test-all \
       hylic-test-hylo hylic-test-hylo-correctness hylic-test-hylo-stress \
       hylic-test-hylo-interleaving hylic-test-hylo-lifts hylic-test-hylo-foldchain \
       hylic-bench hylic-bench-large hylic-bench-modes hylic-bench-overhead hylic-bench-module \
       hylic-bench-hylo hylic-bench-hylo-full hylic-bench-report hylic-bench-full \
       hylic-docs-build hylic-docs-serve

# ── Quick checks ────────────────────────────────────────────
hylic-check:
	@cargo check --lib --tests --benches

# ── Tests ───────────────────────────────────────────────────
hylic-test:
	@cargo test --lib -- --nocapture

hylic-test-parallel:
	@cargo test --test test_eager --test test_eager_debug -- --nocapture

hylic-test-all: hylic-test hylic-test-parallel

# ── Hylo tests (one-stop + individual) ──────────────────────
hylic-test-hylo:
	@cargo test --lib -- --test-threads=1 --nocapture hylomorphic

hylic-test-hylo-correctness:
	@cargo test --lib -- --test-threads=1 --nocapture hylomorphic::tests::correctness

hylic-test-hylo-stress:
	@cargo test --lib -- --test-threads=1 --nocapture hylomorphic::tests::stress

hylic-test-hylo-interleaving:
	@cargo test --lib -- --test-threads=1 --nocapture hylomorphic::tests::interleaving

hylic-test-hylo-lifts:
	@cargo test --lib -- --test-threads=1 --nocapture hylomorphic::tests::lift_compat

hylic-test-hylo-foldchain:
	@cargo test --lib -- --test-threads=1 --nocapture hylomorphic::fold_chain

# ── Benchmarks ──────────────────────────────────────────────
hylic-bench:
	@bash Makefile-scripting/bench-run.sh all

hylic-bench-seq:
	@bash Makefile-scripting/bench-run.sh seq

hylic-bench-par:
	@bash Makefile-scripting/bench-run.sh par

hylic-bench-module:
	@bash Makefile-scripting/bench-run.sh module

hylic-bench-hylo:
	@bash Makefile-scripting/bench-run.sh bench_hylo_compare

hylic-bench-hylo-full: hylic-bench-hylo hylic-bench-report hylic-docs-build
	@echo "Done: hylo benchmark + report + docs rebuilt"

hylic-bench-large:
	@HYLIC_BENCH_SCALE=large bash Makefile-scripting/bench-run.sh all

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
