# RP2040 兼容性评估

**结论：当前仓库环境不可直接用于 RP2040。**

现有实现明显绑定了 ESP32-S3、Arduino-ESP32 CDC/HID API 和 LuatOS ESP32S3-AIO 的板级假设，主机侧还通过固定的 USB Vendor/Product 和板卡名称识别设备；要让 RP2040 可用，必须至少替换设备端 USB 枚举、固件入口与板级 GPIO 约束，不能只改配置或少量脚本。[platformio.ini:1-22][src/main.cpp:1-15][host/text_helper.py:22-25][host/text_helper.py:188-194]

## 1. 当前 ESP32-S3 绑定依据

### 1.1 构建环境和 USB 实现绑定 ESP32-S3

PlatformIO 默认且仅定义 `esp32s3` 环境，使用 `espressif32`、`esp32-s3-devkitc-1`、Arduino framework 和 ESP32 内建 USB/JTAG 上传协议。[platformio.ini:1-22]

固件入口直接使用 Arduino-ESP32 的 `USB`、`USBCDC` 和 `USBHIDKeyboard` API；这些调用不能通过替换 `board` 直接移植到 RP2040。[src/main.cpp:1-15][src/main.cpp:79-87]

主机 helper 只会扫描 `vid == 0x303A` 且产品名为 `ESP Vibe Text Keyboard` 的串口设备；固件还显式设置 PID `0x4002`，但 helper 当前不校验 PID。[host/text_helper.py:22-25][host/text_helper.py:188-194][src/main.cpp:79-82]

### 1.2 设计文档直接写死了 ESP32-S3

设计说明写明“turn the LuatOS ESP32S3-AIO board”并且后续所有 USB、上传、验证流程都以 ESP32-S3 为前提。[docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:3-11][docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:101-121]

### 1.3 GPIO 支持集合是按当前板卡固化的

`GpioTriggerController` 只接受 `0-9` 与 `12-18`，并显式排除了 `10/11/19/20` 等当前板卡上的特殊引脚。[lib/gpio_trigger/src/GpioTriggerController.h:20-23]

测试也把这组 GPIO 当作固定契约来验证。[test/test_gpio_trigger/test_main.cpp:9-18]

### 1.4 配置层也默认沿用这套引脚集合

`config.yaml` 的示例只使用当前受支持的 GPIO，说明现有配置语义与板卡引脚集合是耦合的，而不是可自由迁移的通用定义。[config.yaml:1-7]

## 2. 可复用部分

以下部分迁移到 RP2040 时大概率可以直接保留或仅做小改：

- **按键事件协议**：`PRESS <id> <gpio>` / `PASTE <id>` / `SKIP <id>` 这套主机-设备协议是板卡无关的。[docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:63-93][test/test_gpio_trigger/test_main.cpp:88-109]
- **去抖和 pending 事件逻辑**：`GpioTriggerController` 的去抖、单事件排他、超时释放、响应匹配逻辑都属于通用输入状态机。[lib/gpio_trigger/src/GpioTriggerController.h:18-47][test/test_gpio_trigger/test_main.cpp:20-87][test/test_gpio_trigger/test_main.cpp:112-135]
- **主机侧文本映射与热重载**：`MappingConfig`、`handle_press`、`save_mappings`、TUI 编辑器都主要是数据与 UX 层，不依赖 ESP32-S3 硬件本身。[host/text_helper.py:99-160][host/text_helper.py:174-236][test/test_helper.py:26-80][test/test_helper.py:112-167]
- **UTF-8 文本复制策略**：用 `pbcopy` 写入剪贴板，再由设备发送粘贴快捷键，这套“文本不直接当键盘字符输入”的思路仍然适用。[docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:75-92][host/text_helper.py:170-185]

## 3. 必须迁移/替换的部分

1. **设备 USB 栈与枚举信息**
   - 当前主机侧依赖固定 VID 和产品字符串，PID 只在固件侧配置。[host/text_helper.py:22-25][host/text_helper.py:188-194][src/main.cpp:79-82]
   - RP2040 固件需要配置新的、受授权的 VID/PID/产品字符串，并实现 CDC/HID 复合设备或其它传输方式；USB 标识来自固件描述符，不是 RP2040 板型的固有属性。

2. **板级 GPIO 定义与主机白名单**
   - 固件控制器和主机 helper 都把 `0-9, 12-18` 固化为支持集合。[lib/gpio_trigger/src/GpioTriggerController.h:20-23][host/text_helper.py:22]
   - RP2040 上必须重新定义可接按钮的 GPIO，并同步更新配置校验和 TUI 展示；更稳健的做法是从板卡配置或设备能力信息获取该集合。

3. **上传与开发流程**
   - 当前文档和脚本都按 ESP32-S3 的构建/上传路径描述。[docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:101-110]
   - RP2040 需要新的 PlatformIO target、板定义与烧录流程。

4. **设备名称与自动发现逻辑**
   - 若不改主机 helper，它会继续寻找 ESP32-S3 设备，RP2040 不会被识别。[host/text_helper.py:188-194]

## 4. 推荐的最小改动路线

### 方案：PlatformIO + Arduino-Pico + TinyUSB

这是最适合“尽快让 RP2040 跑起来”的路线，目标是尽量复用当前协议与主机 helper，只替换设备端实现。

#### 建议做法

- 在 PlatformIO 中为选定的 RP2040 板增加独立环境，并明确选择 Earle Philhower Arduino-Pico core；所需 platform 源和配置项应以该 core 当前版本的官方文档为准。
- 显式启用 Arduino-Pico 支持的 TinyUSB 路径（通常涉及 `USE_TINYUSB`），按 Arduino-Pico/Adafruit TinyUSB API 重写 USB 初始化与 CDC+HID 复合描述符；现有 Arduino-ESP32 API 不能直接复用。
- 保留现有 `PRESS/PASTE/SKIP` 协议与事件 ID 机制。
- 设备端继续提供一个 CDC 通道给主机 helper，另一路提供 HID 键盘能力，用于发送 `Cmd+V`/`Ctrl+V`。
- 为固件配置实际使用的 VID/PID/产品字符串，并相应更新主机设备发现逻辑。
- 更新或抽象主机侧 GPIO 白名单及 TUI 列表，使其匹配所选 RP2040 板和接线。

#### 为什么这是“最小改动”

- 主机侧的文本映射、剪贴板和大部分 TUI 逻辑都可保留；设备识别和 GPIO 能力定义仍需修改。[host/text_helper.py:22-25][host/text_helper.py:99-160][host/text_helper.py:197-389]
- 输入状态机可直接复用。[lib/gpio_trigger/src/GpioTriggerController.h:18-47]
- 改动主要集中在设备端 USB/板级层，以及主机侧硬件能力边界，而不必重写整个交互模型。

## 5. 长期路线

### 方案：RP2040 + QMK Raw HID

如果目标是长期维护和更强的键盘固件生态，建议把 RP2040 迁移到 **QMK**，并使用 **Raw HID** 作为主机通信通道。

#### 长期收益

- QMK 对键盘输入、层、组合键、USB 设备行为有成熟支持。
- Raw HID 适合承载当前这类“GPIO 触发 -> 主机文本映射 -> 回传结果”的控制面数据。
- 后续可以把更多输入设备行为统一到 QMK 生态里，减少自维护 USB 代码。

#### 长期取舍

- 迁移成本高于 Arduino-Pico/TinyUSB。
- 主机 helper 需要从 CDC 扫描改为 Raw HID 设备发现与收发，并增加 HIDAPI 类依赖。
- QMK Raw HID 使用固定长度报告（通常由 `RAW_EPSIZE` 定义，常见值为 32 字节），因此需要为消息类型、有效长度、填充、事件 ID 和异步收发定义二进制帧；不能直接照搬换行分隔的可变长度 CDC 文本流。
- Raw HID 是键盘 HID 之外的附加接口，最终粘贴快捷键仍由 QMK 键盘接口发送。
- 但长期可维护性、社区成熟度和可扩展性通常更好。

## 6. 迁移检查清单

### 设备端

- [ ] 选择 RP2040 目标板与具体引脚分配
- [ ] 重新定义支持 GPIO 集合，不再沿用 ESP32-S3 的 `0-9, 12-18`
- [ ] 确认输入电平全部为 3.3V 逻辑
- [ ] 实现去抖、单事件排他、超时释放
- [ ] 实现 USB CDC / Raw HID / HID 键盘输出
- [ ] 验证复位、下载模式、USB 断连恢复流程
- [ ] 更新板卡识别字符串或协议发现方式

### 主机端

- [ ] 更新设备发现逻辑，使其匹配 RP2040 固件实际配置的 USB 描述符
- [ ] 更新或抽象 GPIO 白名单、配置校验和 TUI 展示
- [ ] 若使用 Raw HID，增加 HIDAPI 类依赖并实现固定长度帧的编解码及异步收发
- [ ] 复测 `pbcopy` 粘贴流程在新传输方式下是否仍稳定
- [ ] 复测热重载、无效配置回退、Unicode 文本复制
- [ ] 复测按键事件时序与超时行为

### 配置与文档

- [ ] 更新 `config.yaml` 的示例引脚
- [ ] 更新设计文档中的板卡名称与 USB 说明
- [ ] 更新构建、上传、测试步骤
- [ ] 明确哪些文件保持兼容，哪些已是 RP2040 专用

## 7. 硬件安全注意事项

- **彻底隔离 PSTN 电话线**：原电话线接口必须断开、拆除或封堵，不得连接 USB 电路、USB GND、电源或可触及金属件；改造后不得再接入墙上电话线。传统 PSTN 线路可能带直流馈电和较高振铃电压。[docs/telephone-usb-voice-terminal-mod-guide.md:107-122]
- **隔离原电话 ASIC**：仅关闭原主板电源并不安全，未供电 ASIC 的保护二极管仍可能被 GPIO 反向供电，导致串扰、幽灵按键或损坏。应切断相关走线、移除连接元件、拆除 ASIC 或重制按键板，并在上电前验证不存在反向供电路径。[docs/telephone-usb-voice-terminal-mod-guide.md:434-449]
- **RP2040 GPIO 使用 3.3 V 逻辑**：任何可能高于 3.3 V、带外部供电、长线或来源不明的信号，都应按所选板卡数据手册完成限压、隔离和 ESD/浪涌保护。5 V 逻辑应使用正确计算的分压、专用电平转换器或兼容的开漏接口；单独串联电阻不是通用的 5 V 电平转换方案。
- **重新核对每个 GPIO 的复用功能**，避免占用板载 USB、调试、闪存或启动相关资源；上电前确认 BOOT/恢复路径。
- **按键线路做好保护**：外部按键可按实际线长和干扰环境配置串联保护、去抖及 ESD 器件，并确保公共地和电缆布线可靠。
- **GPIO 不直接驱动负载**：功放使能、继电器或其它负载应通过 MOSFET、三极管或专用驱动器件控制。
- **BTL Class-D 输出不得接地**：PAM8301/PAM8302A 等功放的扬声器只能跨接 `OUT+` 和 `OUT-`，任一输出端都不得接 GND、USB 屏蔽层或其它扬声器负端；输出线应成对且远离麦克风线。[docs/telephone-usb-voice-terminal-mod-guide.md:361-369]
- **控制 USB 供电预算**：若 RP2040、USB Audio、USB HUB 和功放共用单根 USB 供电，应逐项预算并实测峰值电流与最低输入电压，原型整机宜控制在约 450 mA 内，底座功放宜限制在约 0.5–1 W。应验证高音量、冷启动、热插拔和睡眠恢复时不会重启或重新枚举，并按工程需求增加自恢复保险丝、USB ESD 保护和功放近端储能电容。[docs/telephone-usb-voice-terminal-mod-guide.md:588-637]
- **正确连接音频链路**：USB 声卡左右声道不得直接短接；需要单声道时应使用电阻混音或仅取一路。初次连接原听筒受话器时应从低音量和串联限流开始，不能用大功率扬声器输出直接满功率驱动。[docs/telephone-usb-voice-terminal-mod-guide.md:303-334]

## 8. 结论

当前仓库并不是“换一块 RP2040 板子就能直接跑”的结构；它已经把 ESP32-S3 的 USB 枚举、上传流程和 GPIO 选择写进了实现与文档。[host/text_helper.py:22-25][host/text_helper.py:188-194][docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:37-45][docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md:101-118]

**因此，当前不可直接用于 RP2040。**

若以“最小可用迁移”为目标，优先走 **PlatformIO + Arduino-Pico + TinyUSB**；若以“长期维护和生态整合”为目标，再迁移到 **RP2040 + QMK Raw HID**。
