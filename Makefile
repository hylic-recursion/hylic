.PHONY: test bench check

test:
	@bash Makefile-scripting/test.sh

bench:
	@bash Makefile-scripting/bench.sh

check:
	@cargo check --lib
