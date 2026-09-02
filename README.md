# Kivo

把实体按键变成电脑里的文字与快捷键。

[下载最新版本](https://github.com/leowzz/kivo/releases/latest) · [快速上手](#快速上手) · [刷入固件](#刷入固件) · [本地开发](#本地开发)

![小黑操作 Kivo 插线台，把实体按键接成文字和快捷键](assets/readme-illustrations/01-kivo-switchboard.png)

Kivo 是一套由设备固件和 Tauri 桌面 helper 组成的实体按键工作台。它识别每一台控制器，为设备分配对应的按键布局与接线配置，再把按下动作转换成文字粘贴或快捷键。

## 能做什么

- **执行桌面动作**：一个按键可以依次执行文字粘贴和快捷键动作。
- **学习实体接线**：支持独立 GPIO 按键与触点矩阵，并可通过指定设备进行按键学习。
- **管理多台设备**：每台设备保留独立的 Runtime Assignment，切换编辑中的配置不会改动其他设备。
- **复用设备配置**：一个 Device Profile 可以包含多个 Hardware Profile，适配不同板卡或接线版本。
- **功能开关门控**：Hardware Profile 可把一个 GPIO 开关绑定到若干按钮；开关断开时，这些按钮不会执行动作。
- **观察实际使用**：首页展示累计次数、今日次数、活跃按键、七日热力图和最近活动。
- **迁移与恢复**：支持单个设备配置导入导出，以及包含设备分配和统计数据的完整备份恢复。

![Kivo 首页显示设备状态、按键统计和最近活动](assets/readme/app-overview.jpg)

## 快速上手

1. 从 [Releases](https://github.com/leowzz/kivo/releases/latest) 下载 macOS 安装包或 Windows x64 安装程序。
2. 按照[刷入固件](#刷入固件)为受支持的控制器刷入对应固件，然后连接控制器。通过身份与协议校验后，Kivo 会自动登记这台设备。
3. 新建 Device Profile，或从已有配置复制/导入。Device Profile 决定可见按键布局与动作。
4. 为目标板卡创建 Hardware Profile，通过手动配置或学习模式把实体输入映射到按键。
5. 在“按键行为”中配置文字粘贴或快捷键，然后在“设备管理”中保存 Runtime Assignment。
6. 按下实体按键，在首页确认动作、计数和活动记录。

新登记的设备在获得有效 Runtime Assignment 前不会执行动作。编辑中的 Device Profile 也不会自动替换任何设备正在使用的配置。

## 刷入固件

从 [最新 Release](https://github.com/leowzz/kivo/releases/latest) 下载与板卡对应的固件：

| 板卡 | 选择这个文件 |
|---|---|
| YD-ESP32-S3 | `kivo-vX.Y.Z-esp32s3.bin` |
| YD-RP2040 | `kivo-vX.Y.Z-rp2040.uf2` |

### YD-RP2040：拖入文件管理器

1. 让板卡进入 BOOTSEL 模式：
   - 板卡尚未连接时，按住 **BOOT**，插入 USB；看到 `RPI-RP2` 磁盘后松开 **BOOT**。
   - 板卡已经连接时，按住 **BOOT**，短按一次 **RESET**，然后松开 **BOOT**。
2. 在 Finder 或文件资源管理器中打开 `RPI-RP2`。
3. 把 `kivo-vX.Y.Z-rp2040.uf2` 拖进磁盘。复制完成后磁盘会自动退出，板卡会运行 Kivo 固件。

### ESP32-S3：在浏览器中选择固件

ESP32-S3 的下载模式不会显示成磁盘。请使用 Chrome 或 Edge：

1. 下载 `kivo-vX.Y.Z-esp32s3.bin`，打开 Espressif 官方的 [ESP Tool](https://espressif.github.io/esptool-js/)。
2. 按住板卡的 **BOOT**，短按一次 **RESET/RST**，然后松开 **BOOT**。
3. 点击 **Connect**，选择刚出现的 ESP32-S3 串口。
4. 点击 **Add File**，地址填写 `0x0`，选择下载的 `.bin` 文件。
5. 点击 **Program**。完成后短按一次 **RESET/RST**，板卡会运行 Kivo 固件。

只使用上表中与板卡匹配的文件。刷写完成后保持 USB 连接，Kivo 会自动检测设备。

## 配置怎样组合

![小黑把按键布局、硬件接线和设备分配三层描图套准](assets/readme-illustrations/02-profile-registration.png)

| 概念 | 负责什么 |
|---|---|
| Device Profile | 可见布局、按键定义、动作，以及一个或多个 Hardware Profile |
| Hardware Profile | 面向具体板卡的接线拓扑、输入绑定和去抖设置 |
| Device | 一台有稳定硬件序列号的实体控制器；USB 端口不是设备身份 |
| Runtime Assignment | 把一个 Device Profile 和兼容的 Hardware Profile 分配给一台 Device |
| Editor Profile | 当前正在界面中编辑的 Device Profile，不影响其他设备运行 |

功能开关属于 Hardware Profile 的输入源，不会出现在按键布局中。配置时只需选择开关 GPIO 和受影响的按钮。开关闭合时启用按钮；断开或 helper 尚未确认开关状态时屏蔽按钮。被屏蔽的按键不会计入动作统计，正在执行的动作会完整结束。

## 硬件产品命名

Kivo 实体产品使用 `<product-family>-k<key-count>-<capabilities>-r<hardware-revision>`
形式的 Product Version ID。产品能力变体和 PCB 修订彼此独立，软件发布版本、固件
版本、生产批次和单台设备序列号不进入该 ID。

当前规划中的 **Kivo Workbench One** 包含 18 个独立实体按键、麦克风、集成显示屏，
以及可旋转、可按压的编码器，其首版命名为：

```text
workbench-one-k18-mic-disp-encp-r01
```

其中 `k18` 不包含编码器按压，`encp` 明确表示编码器同时支持旋转和按压。该名称
记录的是计划目标，不代表硬件、固件或实体设备已经完成验收。完整字段定义、token
顺序和升级规则见[产品版本 ID 命名规范](docs/product-version-id-naming.md)。

## 支持的控制器

| 板卡 | Controller Family | 运行时 USB | 固件环境 | 上传命令 |
|---|---|---|---|---|
| YD-ESP32-S3 | ESP32-S3 | `303a:4002` | `esp32s3` | `make upload-esp32s3` |
| YD-RP2040 | RP2040 | `2e8a:102e` | `rp2040` | `make upload-rp2040` |

YD-RP2040 的 UF2 bootloader USB 标识为 `2e8a:0003`。Kivo 会先校验 USB 身份，再通过 `HELLO` 协议确认板卡和固件；不受该 Board Profile 支持的 GPIO 会被拒绝。

YD-RP2040 的 Hardware Profile 支持两种地址为 `0x3C` 的 OLED：原有 SSD1306 128x32 模块占用 SDA/SCL 两个 GPIO；`sh1106-1.3-128x64-ec11` 模块使用 SH1106 128x64 屏，并带 EC11 旋转、EC11 按压、确认和返回，共占用七个 GPIO。OLED 和控制面板占用的 GPIO 不会再分配给按键输入或学习模式。

## Codex 状态屏

启用 OLED 的设备会显示本机 Codex 任务的低干扰状态：汇总画面为 `CODEX <N> RUN`，需要操作时显示 `NEEDS INPUT` 或 `APPROVAL NEEDED`，响应生成后短暂显示 `RESPONSE READY`，数据源不可用时显示 `CODEX OFFLINE`。Codex 数据源异常不会停止 Kivo 的按键 Runtime。

Kivo 只消费任务身份、工作目录和状态/生命周期信号；对话正文、推理、工具内容和最终回复不会显示或保留。SSD1306 继续使用原有 `ssd1306` 配置和 128x32 渲染器；SH1106 使用独立的 `sh1106` 配置、128x64 渲染器和协议 11 固件。两种屏均固定为 rotation 0，互不迁移。

刷入固件后仍需分别在实体 SSD1306 和 SH1106 上检查文字与状态切换，并在 SH1106 模块上检查旋钮方向、按压、确认和返回。自动测试和固件构建不能替代物理屏幕与输入验收。

![小黑同时给 ESP32-S3 和 YD-RP2040 两台设备上弦](assets/readme-illustrations/03-parallel-devices.png)

两种控制器共享按键扫描、去抖、协议和运行状态机，各自只保留很薄的 USB/HID 平台适配。多台 ESP32-S3 与 YD-RP2040 可以同时在线，每台设备继续使用自己的 Runtime Assignment。

## 本地开发

需要：

- Node.js `>=24.12.0 <25` 与 npm `>=11.0.0 <12`；`.nvmrc` 和
  `packageManager` 记录 CI 使用的参考版本
- Rust stable 和 Tauri 2 所需的系统构建依赖
- Python `3.13`、[`uv`](https://docs.astral.sh/uv/) 与 PlatformIO
- macOS 或 Windows；发行工作流构建 macOS universal DMG 和 Windows x64 NSIS 安装程序
- 固件相关的 `make` 目标还需要 GNU Make；Windows 可使用 Git for Windows 附带的 shell

```bash
git clone https://github.com/leowzz/kivo.git
cd kivo
cp .env.example .env

nvm install
nvm use
uv sync
npm ci
make client
```

`.env` 会被有意忽略，且只包含 `version=vX.Y.Z`。它为本地固件构建和 `make release` 提供仓库版本。

仓库的 `.envrc` 会加载 `.nvmrc` 中的精确 Node 版本。使用 direnv 时可以验证实际解析到的工具：

```bash
direnv allow
direnv exec . node --version
direnv exec . npm --version
```

预期分别输出 `v24.18.0` 和 `11.16.0`。

Windows PowerShell 不需要 direnv。安装 `.nvmrc` 中的版本后可以直接启动：

```powershell
nvm install 24.18.0
nvm use 24.18.0
npm install --global npm@11.16.0
Copy-Item .env.example .env
uv sync
npm ci
uv run python scripts/kill_helper.py
make
```

本地环境只要落在上述兼容范围内即可，不要求与 CI 的参考版本完全一致。

`make release` 默认递增 patch；`make release V=vX.Y.Z` 可指定版本。脏工作树会被拒绝，跟踪的包版本会以 `chore: release vX.Y.Z` 提交，最后才创建带注释的 tag。

## 固件

分别构建两个固件目标：

```bash
make build-esp32s3
make build-rp2040
```

分别上传；不要使用泛化的 `make upload`：

```bash
make upload-esp32s3
make upload-rp2040
```

Product Studio 生成的生产固件会写入 `output/products/<product-version-id>/<build-id>/`，其中包含固件、产品定义和 `manifest.json`。批量刷写时使用：

```bash
make upload-prod
```

命令会先选择已连接的设备，再扫描与板卡匹配的产品固件；确认后完成刷写，并用固件内嵌的 Product Version ID 和 Build ID 校验启动协议。需要固定设备或固件时可以直接指定：

```bash
make upload-prod SERIAL=E0C9125B0D9B \
  FIRMWARE=output/products/<product-version-id>/<build-id>/firmware.uf2
```

`FIRMWARE` 只能指向 `output/products/` 下与目标板卡匹配的 `.uf2`（RP2040）或 `.bin`（ESP32-S3）产品产物；未指定时会在终端选择器中列出全部可用版本。

当同时连接多块同型号板卡时，用稳定硬件序列号指定目标：

```bash
make upload-esp32s3 SERIAL=ABCDEF123456
make upload-rp2040 SERIAL=E0C9125B0D9B
```

监控固件的 USB CDC 串口（默认 `115200` 波特率）：

```bash
make monitor                 # 默认选择 RP2040
make monitor-rp2040
make monitor-esp32s3
```

未传 `SERIAL` 时会打开设备选择器；也可以直接指定设备和波特率：

```bash
make monitor-rp2040 SERIAL=E0C9125B0D9B
make monitor-esp32s3 SERIAL=ABCDEF123456 BAUD=921600
```

监控命令会先停止可能占用串口的 Kivo helper。按 `Ctrl+C` 退出监控。

## 测试与构建

```bash
make test
make helper-build
```

`make test` 会运行发布脚本测试、Python 上传/选择测试、PlatformIO native 测试、Rust 测试与 Clippy、前端测试和生产构建。`make helper-build` 会连续构建 Kivo 和 Kivo Product Studio 两套包：macOS 生成应用包，Windows 生成对应的 NSIS 安装程序。Windows CI 也会在每次 pull request 中验证两套 NSIS 安装程序。

## 项目结构

```text
src/                 React 配置界面与共享固件入口
src/platform/        ESP32-S3 与 RP2040 的 USB/HID 适配
src-tauri/           设备发现、运行协调、存储、统计与系统托盘
lib/gpio_trigger/    板卡无关的输入拓扑、去抖与协议状态机
models/prod/         随应用发布的 Device Profile
scripts/             固件选择、上传与运行时验证工具
test/                Python、PlatformIO 与发布流程测试
docs/                硬件改造、兼容性与设计记录
```

领域术语以 [`CONTEXT.md`](CONTEXT.md) 为准，实体产品版本命名见
[`docs/product-version-id-naming.md`](docs/product-version-id-naming.md)。电话硬件改造和
电气安全要求见 [`docs/telephone-usb-voice-terminal-mod-guide.md`](docs/telephone-usb-voice-terminal-mod-guide.md)；
改造设备必须彻底隔离原 PSTN 电话线路。

## 平台状态

Kivo 支持 macOS 和 Windows 10/11 x64。Windows 使用原生 Unicode 剪贴板、系统托盘、COM/PnP 设备发现、按硬件身份锁定的 ESP32-S3/RP2040 上传流程，以及 x64 NSIS 安装程序。Windows 安装包目前未做代码签名，首次运行时可能显示系统信誉提示。
