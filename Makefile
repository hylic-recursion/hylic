.PHONY: hylic-check hylic-test hylic-test-parallel hylic-test-all \
       hylic-test-hylo hylic-test-hylo-correctness hylic-test-hylo-stress \
       hylic-test-hylo-interleaving hylic-test-hylo-lifts hylic-test-hylo-foldchain \
       hylic-test-funnel hylic-test-funnel-correctness hylic-test-funnel-stress \
       hylic-bench hylic-bench-compare hylic-bench-full \
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

# ── Hylo tests ──────────────────────────────────────────────
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

# ── Funnel tests ────────────────────────────────────────────
hylic-test-funnel:
	@cargo test --lib -- --test-threads=1 --nocapture hylo_funnel

hylic-test-funnel-correctness:
	@cargo test --lib -- --test-threads=1 --nocapture hylo_funnel::tests::correctness

hylic-test-funnel-stress:
	@cargo test --lib -- --test-threads=1 --nocapture hylo_funnel::tests::stress

# ── Benchmarks ──────────────────────────────────────────────
# Each bench target runs criterion, generates report, rebuilds docs.
#
# bench-compare: funnel vs hylo vs rayon (daily driver)
# bench:         parallel + comparative
# bench-full:    everything

# Atomic bench units (not user-facing)
# Each calls bench-one.sh → streams output, archives raw + criterion + report
_bench-seq:
	@bash Makefile-scripting/bench-one.sh bench_sequential target/bench-latest/sequential
_bench-par:
	@bash Makefile-scripting/bench-one.sh bench_parallel target/bench-latest/parallel
_bench-module:
	@bash Makefile-scripting/bench-one.sh bench_module_sim target/bench-latest/module-sim
_bench-executor:
	@bash Makefile-scripting/bench-one.sh bench_executor_compare target/bench-latest/executor-compare
_bench-hylo:
	@bash Makefile-scripting/bench-one.sh bench_hylo_compare target/bench-latest/hylo-compare

# Copy reports to docs + rebuild
_bench-finish:
	@for d in target/bench-latest/*/report; do \
		cp -r "$$d"/* ../hylic-docs/book/src/bench-results/ 2>/dev/null || true; \
	done
	@$(MAKE) hylic-docs-build

# User-facing targets
hylic-bench-compare: _bench-hylo _bench-finish

hylic-bench: _bench-par _bench-hylo _bench-finish

hylic-bench-full: _bench-seq _bench-par _bench-module _bench-executor _bench-hylo _bench-finish

# Cross-variant comparison (8 git tags, same bench, different source)
hylic-bench-variants:
	@bash _bench-experiment/run-all.sh

# ── Docs ────────────────────────────────────────────────────
hylic-docs-build:
	@bash ../hylic-docs/Makefile-scripting/build-book.sh

hylic-docs-serve:
	@fuser -k 8321/tcp 2>/dev/null || true
	@cd ../hylic-docs/book && mdbook serve -p 8321
