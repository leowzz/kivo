.PHONY: all build download-mode upload test helper helper-kill helper-build release

all: helper

build:
	uv run pio run -e esp32s3

download-mode:
	uv run python scripts/enter_download_mode.py

upload: download-mode
	uv run pio run -e esp32s3 -t upload
	uv run pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 run

test:
	bash test/test_release.sh
	uv run pio test -e native
	cargo test --manifest-path src-tauri/Cargo.toml
	npm test

helper:
	npm run tauri dev

helper-kill:
	@pids="$$(pgrep kivo || true)"; [ -z "$$pids" ] || kill $$pids

helper-build:
	npm run tauri build -- --bundles app

# Bump patch in .env and create annotated git tag. Override version: make release V=v1.2.3
release:
	@ENV_FILE=.env V="$(V)" bash scripts/release.sh
