.PHONY: all build build-esp32s3 build-rp2040 download-mode upload upload-esp32s3 upload-rp2040 require-serial test helper helper-kill helper-build release

BUILD_ID ?= 0.1.0+dev

all: helper

require-serial:
	@test -n "$(SERIAL)" || { echo "SERIAL is required" >&2; exit 2; }

build: build-esp32s3

build-esp32s3:
	KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e esp32s3

build-rp2040:
	KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e rp2040

download-mode: require-serial
	uv run python scripts/enter_download_mode.py --serial "$(SERIAL)"

upload:
	@echo "Specify upload-esp32s3 or upload-rp2040 with SERIAL=<hardware serial>" >&2
	@exit 2

upload-esp32s3: require-serial build-esp32s3
	@download_port="$$(uv run python scripts/enter_download_mode.py --serial "$(SERIAL)")" || exit $$?; \
	  KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e esp32s3 -t upload --upload-port "$$download_port" && \
	  uv run pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 --port "$$download_port" --after hard_reset run
	uv run python scripts/verify_runtime_firmware.py --serial "$(SERIAL)" --vid 0x303a --pid 0x4002 --family esp32s3 --board luatos-esp32s3-aio --build "$(BUILD_ID)"

upload-rp2040: require-serial build-rp2040
	uv run pio pkg exec -p tool-picotool-rp2040-earlephilhower -- picotool load -x .pio/build/rp2040/firmware.uf2 --ser "$(SERIAL)"
	uv run python scripts/verify_runtime_firmware.py --serial "$(SERIAL)" --vid 0x2e8a --pid 0x102e --family rp2040 --board vccgnd-yd-rp2040 --build "$(BUILD_ID)"

test:
	bash test/test_release.sh
	uv run pytest test/test_upload_targeting.py
	uv run pio test -e native
	cargo test --manifest-path src-tauri/Cargo.toml
	cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
	npm test
	npm run build

helper: helper-kill
	npm run tauri dev

helper-kill:
	@pids="$$(pgrep kivo || true)"; [ -z "$$pids" ] || kill $$pids

helper-build:
	npm run tauri build -- --bundles app

# Bump patch in .env and create annotated git tag. Override version: make release V=v1.2.3
release:
	@ENV_FILE=.env V="$(V)" bash scripts/release.sh
