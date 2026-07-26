.PHONY: all build upload test helper

all: upload

build:
	uv run pio run -e esp32s3

upload:
	uv run pio run -e esp32s3 -t upload

test:
	uv run pio test -e native
	uv run python -m unittest discover -s test -p 'test_helper.py' -v

helper:
	uv run python -m host.text_helper
