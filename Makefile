.PHONY: hylic-check hylic-test hylic-test-funnel

hylic-check:
	@cargo check --lib --tests

hylic-test:
	@cargo test --lib -- --nocapture

hylic-test-funnel:
	@cargo test --lib -- --test-threads=1 --nocapture exec::variant::funnel
