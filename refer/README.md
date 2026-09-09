# 硬件参考资料

## CH552G：最丐17+4TPRO机械键盘

- 登记日期：2026-09-09。
- 这是新接入的 CH552G 设备，独立于已有的 ESP32-S3 / RP2040 设备。
- 嘉立创开源项目：[最丐17+4触摸机械键盘 Pro](https://oshwhub.com/yangzen/xing-huo-ji-hua-zui-gai-17-4-chu-mo-ji-xie-jian-pan-pro)。
- 原理图中的项目名称为“最丐17+4TPRO机械键盘”，主板标注 MCU 为 `CH552G`。
- 实物灯光配置：用户确认大部分灯珠未焊接，仅背面标记 `L1` 的一颗灯珠可用。原理图的 20 颗灯为完整设计数量，不代表本机装配；L1 对应的原理图位号和数据接线尚未确认。

原理图：

- [主板](SVG_最丐17+4TPRO机械键盘_2026-09-09/SCH_主板_1-P1_2026-09-09.svg)
- [顶板](SVG_最丐17+4TPRO机械键盘_2026-09-09/SCH_顶板_1-P1_2026-09-09.svg)

驱动分析：[CH552G 按键、RGB 与触控条驱动方案](ch552g-driver-analysis.md)。

官方固件资料：[丐17Touch_Ver202 HEX 静态解析与提取产物](firmware/gai17touch-ver202/README.md)。该文件使用四线按键扫描、五路触摸和单路 PWM 灯光，与本目录 17+4TPRO 原理图不同；实物版本仍需核对。

### 已知设备信息

用户提供的 `wchisp info` 输出：

- 芯片：`CH552[0x5211]`。
- Code Flash：14 KiB；Data EEPROM：128 Bytes。
- Bootloader：`02.50`。
- UID：`08-FC-07-BD-00-00-00-00`。

曾出现找不到 WCH ISP USB 设备的报错，用户重新连接后确认恢复识别，具体原因尚未确定。本记录仅登记硬件资料和识别信息，不代表已完成 Kivo 固件适配或烧录。
