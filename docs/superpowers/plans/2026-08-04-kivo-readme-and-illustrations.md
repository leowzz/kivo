# Kivo README and Illustrations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Chinese, user-first project README with one real Kivo screenshot and three Ian Xiaohei-style explanatory illustrations.

**Architecture:** Keep all README-specific visuals under `assets/`, with generated explanatory art isolated from the real application screenshot. The README uses current repository contracts from `CONTEXT.md`, `Makefile`, `package.json`, `platformio.ini`, and the release workflow; it does not add application behavior or dependencies.

**Tech Stack:** Markdown, Kivo React preview data, Vite, Playwright screenshot capture, PNG assets, built-in image generation or an explicitly approved CLI fallback.

## Global Constraints

- The README is Simplified Chinese first and keeps established domain terms such as Device Profile, Hardware Profile, and Runtime Assignment in English.
- User guidance comes before developer and firmware reference material.
- The only supported board profiles described are `luatos-esp32s3-aio` and `vccgnd-yd-rp2040`.
- Node is exactly `24.18.0`; npm is exactly `11.16.0`; Python requires `>=3.13`.
- Generated illustrations are standalone 16:9 PNG files with a pure white background, black hand-drawn line art, at least 35% white space, sparse red/orange/blue Chinese annotations, and Xiaohei performing the core action.
- Do not use PPT infographic styling, commercial vector illustration, cute mascot styling, formal architecture diagrams, gradients, shadows, paper texture, or a type title in the top-left corner.
- The real screenshot must use the repository preview snapshot and must not imply live hardware verification.
- Physical-device acceptance is outside this documentation task; do not claim it was run.

---

### Task 1: Produce README Visual Assets

**Files:**
- Create: `assets/readme/app-overview.png`
- Create: `assets/readme-illustrations/01-kivo-switchboard.png`
- Create: `assets/readme-illustrations/02-profile-registration.png`
- Create: `assets/readme-illustrations/03-parallel-devices.png`

**Interfaces:**
- Consumes: `src/preview.ts` via the existing browser fallback in `src/App.tsx`; the approved visual descriptions in `docs/superpowers/specs/2026-08-04-kivo-readme-design.md`.
- Produces: Four stable repository-relative PNG paths consumed by `README.md`.

- [ ] **Step 1: Confirm the preview build is healthy before capture**

Run:

```bash
rtk npm run build
```

Expected: command exits `0` and creates the Vite production bundle without TypeScript errors.

- [ ] **Step 2: Start the previewable app and capture the real UI**

Run the development server in a persistent terminal:

```bash
rtk npm run dev -- --host 127.0.0.1
```

Open `http://127.0.0.1:1420`, set the browser viewport to `1120x760`, wait until the preview fallback renders the `Tel001` dashboard, and save a full viewport screenshot to `assets/readme/app-overview.png`.

Expected visible evidence: Kivo navigation, `Tel001`, connection state, cumulative/today metrics, heatmap, and recent activity. The capture must not contain a browser error overlay, clipped navigation, or overlapping text.

- [ ] **Step 3: Generate the Kivo switchboard illustration**

Use one standalone image-generation call. Use the built-in image tool when available. If it is unavailable, stop and obtain explicit approval before using the image-generation CLI fallback.

Prompt:

```text
Use case: illustration-story
Asset type: Kivo README hero illustration
Primary request: Generate one standalone 16:9 horizontal Chinese article illustration about Kivo turning physical keypad input into desktop text paste and hotkey actions.
Scene/backdrop: pure white background with a large quiet empty area; no room or environmental background.
Subject: 小黑, a small solid-black absurd creature with white dot eyes, tiny thin legs, a blank serious expression, and a slightly uneven hand-drawn body, is the operator of an odd low-tech telephone switchboard. A coiled telephone cable and a few physical keypad buttons enter the switchboard from one side. Xiaohei actively plugs and turns the switchboard controls. From the other side emerge one short paper strip representing pasted text and one simple lightning-shaped hotkey mark.
Style/medium: minimalist black hand-drawn line art, slightly wobbly pen strokes, clean absurd product sketch, not cute.
Composition/framing: wide 16:9 composition; the switchboard and Xiaohei occupy about half the canvas; preserve at least 35% blank white space; one central action only.
Color palette: black line art; orange only for the cable path; red only for the output emphasis; blue only for one quiet system note.
Text (verbatim): "实体按键" / "Kivo" / "文字" / "快捷键"
Constraints: Xiaohei must perform the core switching action. Use at most four short handwritten Chinese labels. No title in the top-left corner. Invent this composition for Kivo.
Avoid: PPT infographic, formal workflow diagram, commercial vector art, children's illustration, cute mascot poster, realistic UI, complex architecture, gradients, shadows, paper texture, beige background, watermark.
```

Save the selected final as `assets/readme-illustrations/01-kivo-switchboard.png`. Do not overwrite a pre-existing file; if the path already exists, save a `-v2` sibling and update the README path in Task 2.

- [ ] **Step 4: Generate the profile registration illustration**

Use one standalone image-generation call with this prompt:

```text
Use case: illustration-story
Asset type: Kivo README explanatory illustration
Primary request: Generate one standalone 16:9 horizontal Chinese article illustration explaining that a Kivo Device Profile, Hardware Profile, and Runtime Assignment are distinct layers that align into one working physical device.
Scene/backdrop: pure white background with extensive blank space and no literal application interface.
Subject: 小黑, the solid-black deadpan creature with white dot eyes and thin legs, operates two registration pegs and carefully rotates three loose translucent tracing sheets into alignment over a simple physical keypad silhouette. The sheets are visibly different: one carries a sparse button outline, one carries a few hand-drawn wire contacts, and one carries a small device tag. When Xiaohei locks the pegs, one orange motion line ends at a tiny red working pulse.
Style/medium: minimalist black hand-drawn product sketch with slightly wobbly pen lines; strange, serious, clean, not cute.
Composition/framing: wide 16:9; the three offset tracing sheets and Xiaohei occupy 45%-55% of the canvas; preserve at least 35% blank white space; no neat grid or boxes.
Color palette: black main lines; orange only for alignment motion; red only for the final working pulse; blue only for the wiring sheet note.
Text (verbatim): "长什么样" / "怎么接线" / "交给哪台设备" / "开始工作"
Constraints: Xiaohei must actively align and lock the layers. Use only these four short handwritten labels. Do not write Device Profile, Hardware Profile, Runtime Assignment, or a diagram title on the image.
Avoid: formal layered architecture diagram, PPT slide, commercial vector art, cute character, dense arrows, gradients, shadows, paper texture, beige background, watermark, prior case-study compositions.
```

Save the selected final as `assets/readme-illustrations/02-profile-registration.png`, following the same non-overwrite rule.

- [ ] **Step 5: Generate the parallel devices illustration**

Use one standalone image-generation call with this prompt:

```text
Use case: illustration-story
Asset type: Kivo README explanatory illustration
Primary request: Generate one standalone 16:9 horizontal Chinese article illustration showing an ESP32-S3 device and a YD-RP2040 device operating at the same time while keeping independent assignments.
Scene/backdrop: pure white background, no room, no realistic electronics bench.
Subject: 小黑, a solid-black serious creature with white dot eyes and thin legs, stands between two odd hand-cranked record players and actively winds both with a double-ended crank. The left record player has a tiny ESP32-S3 board-shaped token and follows one loose orange groove. The right has a tiny YD-RP2040 token and follows a different loose orange groove. Both grooves end at one small hand-drawn local tally notebook without merging their records.
Style/medium: sparse black hand-drawn line art, wobbly pen, absurd low-tech product sketch, deadpan and not cute.
Composition/framing: wide 16:9; asymmetrical pair rather than a formal comparison chart; Xiaohei and the two machines occupy no more than 60% of the frame; at least 35% blank white space.
Color palette: black main lines; orange for the two independent grooves; red for two separate active marks; blue for the local tally note.
Text (verbatim): "ESP32-S3" / "YD-RP2040" / "各跑各的" / "一起在线"
Constraints: Xiaohei's double-ended winding action is essential to both machines operating. Four short handwritten labels only. The two paths remain visibly independent.
Avoid: architecture diagram, comparison table, PPT infographic, commercial vector illustration, cute mascot, dense circuitry, gradients, shadows, paper texture, beige background, watermark.
```

Save the selected final as `assets/readme-illustrations/03-parallel-devices.png`, following the same non-overwrite rule.

- [ ] **Step 6: Inspect and validate all four images**

Open every PNG at original detail. For each illustration, verify: clean white background, 16:9 dimensions, Xiaohei as the action owner, no top-left type title, no more than four labels, no severe Chinese text errors, at least 35% visible white space, and no PPT/vector/cute styling. Regenerate a failing image with one targeted correction and re-check it.

Run:

```bash
rtk proxy file assets/readme/app-overview.png assets/readme-illustrations/*.png
rtk proxy sips -g pixelWidth -g pixelHeight assets/readme/app-overview.png assets/readme-illustrations/*.png
```

Expected: all files are readable PNGs; each illustration has an exact `16:9` pixel ratio; the app screenshot is `1120x760`.

- [ ] **Step 7: Commit the visual assets**

```bash
rtk git add assets/readme/app-overview.png assets/readme-illustrations
rtk git diff --cached --check
rtk git commit -m "docs: add readme visuals"
```

Expected: one commit containing only the four README image assets.

---

### Task 2: Write and Verify the User-First README

**Files:**
- Create: `README.md`
- Verify: `assets/readme/app-overview.png`
- Verify: `assets/readme-illustrations/01-kivo-switchboard.png`
- Verify: `assets/readme-illustrations/02-profile-registration.png`
- Verify: `assets/readme-illustrations/03-parallel-devices.png`

**Interfaces:**
- Consumes: the four stable image paths from Task 1; current domain and command contracts from the repository.
- Produces: the root `README.md` used by GitHub and local readers.

- [ ] **Step 1: Create the complete README**

Create `README.md` with this content:

````markdown
# Kivo

把实体按键变成电脑里的文字与快捷键。

[下载最新版本](https://github.com/leowzz/kivo/releases/latest) · [快速上手](#快速上手) · [本地开发](#本地开发)

![小黑操作 Kivo 插线台，把实体按键接成文字和快捷键](assets/readme-illustrations/01-kivo-switchboard.png)

Kivo 是一套由设备固件和 Tauri 桌面 helper 组成的实体按键工作台。它识别每一台控制器，为设备分配对应的按键布局与接线配置，再把按下动作转换成文字粘贴或快捷键。

## 能做什么

- **执行桌面动作**：一个按键可以依次执行文字粘贴和快捷键动作。
- **学习实体接线**：支持独立 GPIO 按键与触点矩阵，并可通过指定设备进行按键学习。
- **管理多台设备**：每台设备保留独立的 Runtime Assignment，切换编辑中的配置不会改动其他设备。
- **复用设备配置**：一个 Device Profile 可以包含多个 Hardware Profile，适配不同板卡或接线版本。
- **观察实际使用**：首页展示累计次数、今日次数、活跃按键、七日热力图和最近活动。
- **迁移与恢复**：支持单个设备配置导入导出，以及包含设备分配和统计数据的完整备份恢复。

![Kivo 首页显示设备状态、按键统计和最近活动](assets/readme/app-overview.png)

## 快速上手

1. 从 [Releases](https://github.com/leowzz/kivo/releases/latest) 下载 macOS 安装包或 Windows x64 安装程序。
2. 连接已经刷入 Kivo 固件的受支持控制器。通过身份与协议校验后，Kivo 会自动登记这台设备。
3. 新建 Device Profile，或从已有配置复制/导入。Device Profile 决定可见按键布局与动作。
4. 为目标板卡创建 Hardware Profile，通过手动配置或学习模式把实体输入映射到按键。
5. 在“按键行为”中配置文字粘贴或快捷键，然后在“设备管理”中保存 Runtime Assignment。
6. 按下实体按键，在首页确认动作、计数和活动记录。

新登记的设备在获得有效 Runtime Assignment 前不会执行动作。编辑中的 Device Profile 也不会自动替换任何设备正在使用的配置。

## 配置怎样组合

![小黑把按键布局、硬件接线和设备分配三层描图套准](assets/readme-illustrations/02-profile-registration.png)

| 概念 | 负责什么 |
|---|---|
| Device Profile | 可见布局、按键定义、动作，以及一个或多个 Hardware Profile |
| Hardware Profile | 面向具体板卡的接线拓扑、输入绑定和去抖设置 |
| Device | 一台有稳定硬件序列号的实体控制器；USB 端口不是设备身份 |
| Runtime Assignment | 把一个 Device Profile 和兼容的 Hardware Profile 分配给一台 Device |
| Editor Profile | 当前正在界面中编辑的 Device Profile，不影响其他设备运行 |

## 支持的控制器

| 板卡 | Controller Family | 运行时 USB | 固件环境 | 上传命令 |
|---|---|---|---|---|
| LuatOS ESP32-S3-AIO | ESP32-S3 | `303a:4002` | `esp32s3` | `make upload-esp32s3` |
| VCC-GND YD-RP2040 | RP2040 | `2e8a:102e` | `rp2040` | `make upload-rp2040` |

YD-RP2040 的 UF2 bootloader USB 标识为 `2e8a:0003`。Kivo 会先校验 USB 身份，再通过 `HELLO` 协议确认板卡和固件；不受该 Board Profile 支持的 GPIO 会被拒绝。

![小黑同时给 ESP32-S3 和 YD-RP2040 两台设备上弦](assets/readme-illustrations/03-parallel-devices.png)

两种控制器共享按键扫描、去抖、协议和运行状态机，各自只保留很薄的 USB/HID 平台适配。多台 ESP32-S3 与 YD-RP2040 可以同时在线，每台设备继续使用自己的 Runtime Assignment。

## 本地开发

需要：

- Node.js `24.18.0` 与 npm `11.16.0`
- Rust stable 和 Tauri 2 所需的系统构建依赖
- Python `3.13`、[`uv`](https://docs.astral.sh/uv/) 与 PlatformIO
- macOS 或 Windows；发行工作流构建 macOS universal DMG 和 Windows x64 NSIS 安装程序

```bash
git clone git@github.com:leowzz/kivo.git
cd kivo

nvm install
nvm use
uv sync
npm ci
make helper
```

仓库的 `.envrc` 会加载 `.nvmrc` 中的精确 Node 版本。使用 direnv 时可以验证实际解析到的工具：

```bash
direnv allow
direnv exec . node --version
direnv exec . npm --version
```

预期分别输出 `v24.18.0` 和 `11.16.0`。

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

当同时连接多块同型号板卡时，用稳定硬件序列号指定目标：

```bash
make upload-esp32s3 SERIAL=ABCDEF123456
make upload-rp2040 SERIAL=E0C9125B0D9B
```

## 测试与构建

```bash
make test
make helper-build
```

`make test` 会运行发布脚本测试、Python 上传/选择测试、PlatformIO native 测试、Rust 测试与 Clippy、前端测试和生产构建。`make helper-build` 在本机构建 Tauri 应用包。

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

领域术语以 [`CONTEXT.md`](CONTEXT.md) 为准。电话硬件改造和电气安全要求见 [`docs/telephone-usb-voice-terminal-mod-guide.md`](docs/telephone-usb-voice-terminal-mod-guide.md)；改造设备必须彻底隔离原 PSTN 电话线路。
````

- [ ] **Step 2: Check README paths, anchors, and command sources**

Run:

```bash
rtk proxy test -f README.md
rtk proxy test -f assets/readme/app-overview.png
rtk proxy test -f assets/readme-illustrations/01-kivo-switchboard.png
rtk proxy test -f assets/readme-illustrations/02-profile-registration.png
rtk proxy test -f assets/readme-illustrations/03-parallel-devices.png
rtk grep -n "make helper\|make build-esp32s3\|make build-rp2040\|make upload-esp32s3\|make upload-rp2040" README.md Makefile
rtk grep -n "24.18.0\|11.16.0\|node-version-file" README.md .nvmrc package.json .github/workflows/release-windows.yml
```

Expected: all file checks exit `0`; every documented Make target exists; the exact Node/npm versions match repository contracts; the release workflow consumes `.nvmrc`.

- [ ] **Step 3: Run proportional verification**

Run:

```bash
rtk npm test
rtk npm run build
rtk git diff --check
rtk git status --short
```

Expected: frontend tests and build pass; diff check reports no errors; status contains only the intended README before staging.

- [ ] **Step 4: Review the rendered README and commit**

Render or preview `README.md` and confirm that each image loads, Chinese text is readable, tables fit, code blocks are closed, relative links resolve, and the document remains useful without loading external assets.

Then run:

```bash
rtk git add README.md
rtk git diff --cached --check
rtk git commit -m "docs: add Kivo project readme"
rtk git status --short
```

Expected: one README-only commit and a clean working tree.
