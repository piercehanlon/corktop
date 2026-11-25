.PHONY: setup ci full
setup:
	python3 scripts/ci.py --setup
ci:
	python3 scripts/ci.py
full:
	python3 scripts/ci.py --full
