.PHONY: all build upload test helper helper-build release

all: helper

build:
	uv run pio run -e esp32s3

upload:
	uv run pio run -e esp32s3 -t upload

test:
	bash test/test_release.sh
	uv run pio test -e native
	cargo test --manifest-path src-tauri/Cargo.toml
	npm test

helper:
	npm run tauri dev

helper-build:
	npm run tauri build -- --bundles app

# Bump patch in .env and create annotated git tag. Override version: make release V=v1.2.3
release:
	@ENV_FILE=.env V="$(V)" bash scripts/release.sh
