.PHONY: all dev clean build build-esp32s3 build-rp2040 build-product download-mode upload upload-esp32s3 upload-rp2040 upload-prod monitor monitor-esp32s3 monitor-rp2040 require-build-id require-product validate-env-build-id require-serial test client helper studio helper-kill helper-build helper-build-app helper-build-studio release

ENV_FILE ?= .env
ifeq ($(origin BUILD_ID),undefined)
BUILD_ID = $(shell $(PYTHON) scripts/repo_version.py get --env-file "$(ENV_FILE)")
require-build-id: validate-env-build-id
endif
PYTHON ?= python3
UV ?= uv
UV_CMD = "$(UV)"
BAUD ?= 115200
PRODUCT_OUTPUT_DIR ?= output/products
FIRMWARE ?=
ESP32S3_BUILD = KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" $(UV_CMD) run pio run -e esp32s3
RP2040_BUILD = KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" $(UV_CMD) run pio run -e rp2040

all: client

dev: client

clean:
	rm -rf src-tauri/target/release/bundle/macos/Kivo.app \
		"src-tauri/target/release/bundle/macos/Kivo Product Studio.app"

require-serial:
	@test -n "$(SERIAL)" || { echo "SERIAL is required" >&2; exit 2; }

require-product:
	@test -n "$(PRODUCT)" || { echo "PRODUCT is required" >&2; exit 2; }

validate-env-build-id:
	@$(PYTHON) scripts/repo_version.py get --env-file "$(ENV_FILE)" >/dev/null

require-build-id:
	@case "$(BUILD_ID)" in \
	  ""|*[[:space:]]*) \
	    echo "BUILD_ID is required; run: cp .env.example .env (PowerShell: Copy-Item .env.example .env)" >&2; \
	    exit 2; \
	    ;; \
	esac

build: build-rp2040

build-esp32s3: require-build-id
	$(ESP32S3_BUILD)

build-rp2040: require-build-id
	$(RP2040_BUILD)

build-product: require-build-id require-product
	KIVO_REPOSITORY_ROOT="$$(pwd -P)" KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" cargo run --manifest-path src-tauri/Cargo.toml --features product-studio,product-cli --bin kivo-product -- build "$(PRODUCT)"

download-mode: require-serial
	$(UV_CMD) run python scripts/enter_download_mode.py --serial "$(SERIAL)"

upload: upload-rp2040

upload-esp32s3: helper-kill require-build-id
	@set -e; \
	  serial="$(SERIAL)"; \
	  if [ -z "$$serial" ]; then \
	    serial="$$($(UV_CMD) run python scripts/select_firmware_target.py --board yd-esp32-s3 --mode runtime)"; \
	  fi; \
	  test -n "$$serial" || { echo "SERIAL is required" >&2; exit 2; }; \
	  $(ESP32S3_BUILD); \
	  download_port="$$($(UV_CMD) run python scripts/enter_download_mode.py --serial "$$serial")"; \
	  KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" $(UV_CMD) run pio run -e esp32s3 -t upload --upload-port "$$download_port"; \
	  $(UV_CMD) run pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 --port "$$download_port" --after hard_reset run; \
	  $(UV_CMD) run python scripts/verify_runtime_firmware.py --serial "$$serial" --vid 0x303a --pid 0x4002 --family esp32s3 --board yd-esp32-s3 --build "$(BUILD_ID)"

upload-rp2040: helper-kill require-build-id
	@set -e; \
	  serial="$(SERIAL)"; \
	  if [ -z "$$serial" ]; then \
	    serial="$$($(UV_CMD) run python scripts/select_firmware_target.py --board yd-rp2040 --mode runtime --mode bootloader)"; \
	  fi; \
	  test -n "$$serial" || { echo "SERIAL is required" >&2; exit 2; }; \
	  $(RP2040_BUILD); \
	  runtime_serial="$$($(UV_CMD) run python scripts/upload_rp2040.py --serial "$$serial" --firmware .pio/build/rp2040/firmware.uf2)"; \
	  test -n "$$runtime_serial" || { echo "RP2040 runtime serial is required" >&2; exit 2; }; \
	  $(UV_CMD) run python scripts/verify_runtime_firmware.py --serial "$$runtime_serial" --vid 0x2e8a --pid 0x102e --family rp2040 --board yd-rp2040 --build "$(BUILD_ID)"

upload-prod: helper-kill
	@set -e; \
	  selected_target=""; \
	  if [ -n "$(SERIAL)" ]; then \
	    selected_target="$$($(UV_CMD) run python scripts/select_firmware_target.py --board all --mode runtime --mode bootloader --output target --serial "$(SERIAL)")"; \
	  else \
	    selected_target="$$($(UV_CMD) run python scripts/select_firmware_target.py --board all --mode runtime --mode bootloader --output target)"; \
	  fi; \
	  test -n "$$selected_target" || { echo "No upload target selected" >&2; exit 2; }; \
	  board="$${selected_target%% *}"; \
	  serial="$${selected_target#* }"; \
	  test -n "$$board" -a -n "$$serial" || { echo "Invalid upload target: $$selected_target" >&2; exit 2; }; \
	  firmware_override="$(FIRMWARE)"; \
	  if [ -n "$$firmware_override" ]; then \
	    firmware="$$($(UV_CMD) run python scripts/select_product_firmware.py --root "$(PRODUCT_OUTPUT_DIR)" --board "$$board" --firmware "$$firmware_override")"; \
	  else \
	    firmware="$$($(UV_CMD) run python scripts/select_product_firmware.py --root "$(PRODUCT_OUTPUT_DIR)" --board "$$board")"; \
	  fi; \
	  test -n "$$firmware" || { echo "No product firmware selected" >&2; exit 2; }; \
	  build_id="$${firmware%/*}"; \
	  build_id="$${build_id##*/}"; \
	  product_version_id="$${firmware%/*/*}"; \
	  product_version_id="$${product_version_id##*/}"; \
	  case "$$board" in \
	    yd-rp2040) \
	      runtime_serial="$$($(UV_CMD) run python scripts/upload_rp2040.py --serial "$$serial" --firmware "$$firmware")"; \
	      test -n "$$runtime_serial" || { echo "RP2040 runtime serial is required" >&2; exit 2; }; \
	      $(UV_CMD) run python scripts/verify_runtime_firmware.py --serial "$$runtime_serial" --vid 0x2e8a --pid 0x102e --family rp2040 --board yd-rp2040 --build "$$build_id" --product-version-id "$$product_version_id"; \
	      ;; \
	    yd-esp32-s3) \
	      download_port="$$($(UV_CMD) run python scripts/enter_download_mode.py --serial "$$serial")"; \
	      $(UV_CMD) run pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 --port "$$download_port" --after hard_reset write_flash 0x0 "$$firmware"; \
	      $(UV_CMD) run python scripts/verify_runtime_firmware.py --serial "$$serial" --vid 0x303a --pid 0x4002 --family esp32s3 --board yd-esp32-s3 --build "$$build_id" --product-version-id "$$product_version_id"; \
	      ;; \
	    *) \
	      echo "Unsupported product firmware board: $$board" >&2; \
	      exit 2; \
	      ;; \
	  esac

monitor: monitor-rp2040

monitor-esp32s3: helper-kill
	@set -e; \
	  serial="$(SERIAL)"; \
	  if [ -z "$$serial" ]; then \
	    serial="$$($(UV_CMD) run python scripts/select_firmware_target.py --board yd-esp32-s3 --mode runtime)"; \
	  fi; \
	  port="$$($(UV_CMD) run python scripts/resolve_firmware_port.py --board yd-esp32-s3 --serial "$$serial")"; \
	  echo "Monitoring YD-ESP32-S3 $$serial on $$port at $(BAUD) baud"; \
	  $(UV_CMD) run pio device monitor --port "$$port" --baud "$(BAUD)"

monitor-rp2040: helper-kill
	@set -e; \
	  serial="$(SERIAL)"; \
	  if [ -z "$$serial" ]; then \
	    serial="$$($(UV_CMD) run python scripts/select_firmware_target.py --board yd-rp2040 --mode runtime)"; \
	  fi; \
	  port="$$($(UV_CMD) run python scripts/resolve_firmware_port.py --board yd-rp2040 --serial "$$serial")"; \
	  echo "Monitoring YD-RP2040 $$serial on $$port at $(BAUD) baud"; \
	  $(UV_CMD) run pio device monitor --port "$$port" --baud "$(BAUD)"

test:
	bash test/test_release.sh
	bash test/test_studio_bundle.sh
	$(UV_CMD) run pytest test/test_repo_version.py test/test_release_transaction.py test/test_platformio_build_id.py
	$(UV_CMD) run pytest test/test_upload_targeting.py test/test_rp2040_upload.py
	$(UV_CMD) run pytest test/test_firmware_target_selector.py test/test_product_firmware_selector.py test/test_make_upload_selection.py
	$(UV_CMD) run pytest test/test_monitor.py
	$(UV_CMD) run pio test -e native
	cargo test --manifest-path src-tauri/Cargo.toml
	cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
	npm test
	npm run build

client: helper-kill
	npm run tauri dev

helper: client

studio: helper-kill
	KIVO_REPOSITORY_ROOT="$$(pwd -P)" npm run tauri -- dev --features product-studio --config src-tauri/tauri.studio.conf.json

helper-kill:
	@$(UV_CMD) run python scripts/kill_helper.py

kill: helper-kill

helper-build: helper-build-studio

helper-build-app:
	npm run tauri build

helper-build-studio: helper-build-app
	npm run tauri build -- --features product-studio --config src-tauri/tauri.studio.conf.json

# Bump patch in .env and create annotated git tag. Override version: make release V=v1.2.3
release:
	@ENV_FILE=.env V="$(V)" bash scripts/release.sh
