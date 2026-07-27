.PHONY: all build upload test helper helper-build

all: upload

build:
	uv run pio run -e esp32s3

upload:
	uv run pio run -e esp32s3 -t upload

test:
	uv run pio test -e native
	cargo test --manifest-path src-tauri/Cargo.toml
	npm test

helper:
	npm run tauri dev

helper-build:
	npm run tauri build -- --bundles app
