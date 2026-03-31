.PHONY: test bench bench-report check

test:
	@bash Makefile-scripting/test.sh

bench:
	@bash Makefile-scripting/bench-run.sh

bench-report:
	@python3 Makefile-scripting/bench-report.py

check:
	@cargo check --lib
