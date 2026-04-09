.PHONY: hylic-check hylic-test hylic-test-parallel hylic-test-all \
       hylic-test-hylo hylic-test-hylo-correctness hylic-test-hylo-stress \
       hylic-test-hylo-interleaving hylic-test-hylo-lifts hylic-test-hylo-foldchain \
       hylic-test-funnel hylic-test-funnel-correctness hylic-test-funnel-stress \
       hylic-docs-build hylic-docs-serve

# ── Quick checks ────────────────────────────────────────────
hylic-check:
	@cargo check --lib --tests

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

# ── Docs ────────────────────────────────────────────────────
hylic-docs-build:
	@bash ../hylic-docs/Makefile-scripting/build-book.sh

hylic-docs-serve:
	@fuser -k 8321/tcp 2>/dev/null || true
	@cd ../hylic-docs/book && mdbook serve -p 8321
