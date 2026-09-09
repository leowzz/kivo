# 硬件参考资料

## CH552G 17 键设备

登记及更新日期：2026-09-09。这是独立于已有 ESP32-S3 / RP2040 的新设备。设备范围以用户确认的实物为准，驱动参考使用用户提供的官方 `丐17Touch_Ver202.hex`。

### 实物与固件依据

| 项目 | 已知信息 | 证据范围 |
|---|---|---|
| 主控 | CH552G | 用户确认型号；ISP 识别为 CH552 |
| 输入 | 17 个按键、一条触控条 | 用户确认的实物配置 |
| 灯光 | 大部分灯珠未焊，仅背面 L1 可用 | 用户确认；颜色能力和实际接线未测量 |
| 按键驱动 | P3.0–P3.3 四线扫描 + RST 独立键 | Ver202 静态反汇编 |
| 触摸驱动 | TIN1、TIN5、TIN4、TIN3、TIN2 共五路 | Ver202 静态反汇编；空间顺序未核实 |
| 灯光驱动 | PWM2 单通道，默认 P3.4 | Ver202 静态反汇编；不能视为已确认 L1 接线 |
| 运行时 USB | `3062:4700`，名称 `丐17Touch` | Ver202 描述符；当前实机运行状态未核实 |

当前灯光功能按单颗 LED 的开关、亮度和状态提示分析，不声明已确认 RGB 三色控制。本设备文档不采用 TPRO 原理图中的 21 键、20 颗灯或 USB Hub 作为实际配置。

- [当前驱动方案](ch552g-driver-analysis.md)：四线按键扫描、五路触摸、单路 PWM 灯光和 HID 接入。
- [Ver202 固件解析](firmware/gai17touch-ver202/README.md)：USB 描述符、键码表、厂商命令、代码地址及提取产物。
- [提取数据 YAML](firmware/gai17touch-ver202/extracted.yaml)：可复现的机器可读数据。

官方 HEX 来自用户指定路径 `/Users/leo/Downloads/17键/固件/丐17Touch_Ver202.hex`。该文件不是实机 Flash 读取结果，不包含本机保存的改键、校准参数或实际芯片配置位。

### 开源资料与版本关系

- [最丐17Touch 项目](https://oshwhub.com/yANgZEN/zui-gai-shuo-zi-jian-pan)：Ver202 内嵌的网址，与固件名称和驱动代码对应。
- [最丐17+4TPRO 项目](https://oshwhub.com/yangzen/xing-huo-ji-hua-zui-gai-17-4-chu-mo-ji-xie-jian-pan-pro)：用户最初提供的参考链接；本目录保存的原理图来自该版本。

保存的图纸标题为“最丐17+4TPRO机械键盘”，主板 MCU 标注 CH552G，但其五线按键、四路触摸、P1.5 RGB 灯链与 Ver202 不一致。仅作为对照参考，不能据此认定实物属于 TPRO 或直接使用其接线：

- [TPRO 主板参考图](SVG_最丐17+4TPRO机械键盘_2026-09-09/SCH_主板_1-P1_2026-09-09.svg)
- [TPRO 顶板参考图](SVG_最丐17+4TPRO机械键盘_2026-09-09/SCH_顶板_1-P1_2026-09-09.svg)

### 已知 ISP 信息

用户提供的 `wchisp info` 输出：

- 芯片：`CH552[0x5211]`。
- Code Flash：14 KiB；Data EEPROM：128 Bytes。
- Bootloader：`02.50`。
- UID：`08-FC-07-BD-00-00-00-00`。

曾出现找不到 WCH ISP USB 设备的报错，用户重连后确认恢复，原因未确定。资料登记和 HEX 解析不代表已完成 Kivo 固件适配、烧录或实机驱动验收。
