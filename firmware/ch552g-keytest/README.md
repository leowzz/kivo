# CH552G 临时按键与触摸测试固件

仅用于当前 17 键设备的硬件核对，独立于 Kivo 的 ESP32-S3 / RP2040 固件。引脚依据是 [Ver202 解析](../../refer/firmware/gai17touch-ver202/README.md)，不是 TPRO 参考原理图。2026-09-09 已编译、刷入并通过代码校验，实机识别到键盘和 CDC，记录到全部 17 个按键的按下/松开及五路触摸数据。用户完成逐键、分段滑动与长按测试后确认“没问题”。

## 测试行为

- 每次按下输出对应编号 `1`～`17`，随后一个空格。例如编号 11 输入 `11 `；使用主键盘数字键码，不依赖 NumLock。长按不重复输出，松开后可再次触发。
- 编号 1～16 沿用 Ver202 扫描函数的顺序，17 是 RST 独立键；不代表 PCB S 位号或空间排列。RST 的外部复位功能需要处于关闭状态；固件本身不修改配置位。
- 扫描 P3.0～P3.3，逐键连续 5 次稳定后确认，目标扫描间隔 1 ms。开漏释放并使用上拉，保留 P3 其余位；多键组合仍受实际扫描网络限制。
- 触摸 C1～C5 分别采样 P1.1、P1.7、P1.6、P1.5、P1.4，每路设置 2 ms 采样周期，由触摸中断保存采样，主循环取得快照。实际全帧周期包含主循环调度时间。
- 启动或手动校准时先丢弃 10 帧，再平均 32 帧建立基线；之后固定基线，便于观察长按、松开和漂移。基线仅存 RAM。
- USB 同时提供标准键盘和 CDC 调试串口。串口输出原始采样、基线、按键按下/松开和队列溢出计数，触摸快照目标频率 20 Hz。串口主机不读取时，扫描循环不等待发送完成。
- 本固件不控制 L1，不写 EEPROM，不改芯片配置位。按用户要求替换应用程序，保留官方 HEX 供恢复。

## 编号对应扫描位置

| 状态 | 输入脚顺序 | 输出编号 |
|---|---|---|
| 四线释放，对地按键 | P3.0、P3.1、P3.2、P3.3 | 1、2、3、4 |
| P3.0 拉低 | P3.1、P3.2、P3.3 | 5、6、7 |
| P3.1 拉低 | P3.0、P3.2、P3.3 | 8、9、10 |
| P3.2 拉低 | P3.0、P3.1、P3.3 | 11、12、13 |
| P3.3 拉低 | P3.0、P3.1、P3.2 | 14、15、16 |
| RST 高电平 | CLOCK_CFG.bit3 | 17 |

## 编译与刷入

在仓库根目录执行。构建脚本将固定版本依赖保存在 `~/.cache/kivo-ch552`，当前编译包为 macOS x86_64，在本机 Apple Silicon 的兼容环境下验证通过。

```sh
rtk proxy python3 firmware/ch552g-keytest/build.py
rtk proxy uv run --no-project --with pyserial python firmware/ch552g-keytest/verify.py
rtk proxy python3 firmware/ch552g-keytest/flash.py
```

产物为 [build/keytest.hex](build/keytest.hex)。`flash.py` 仅接受 CH552 / UID `08-FC-07-BD-00-00-00-00`：先保存 ISP 信息和 128 字节 EEPROM，再刷写并校验，随后比较 EEPROM 与配置读数的前 10 字节，确认未变后退出 ISP。设备不在线或身份不符时不会擦写。ISP 回包偏移 10–11 在首次擦写前的只读会话中就发生变化，不参与配置比较；完整原始回包仍保留在证据文件内。现场证据保存在忽略版本控制的 `captures/flash-*`。

USB 运行身份为 `1209:C55C`（沿用 ch55xduino CDC+HID 示例 ID），产品 `Kivo CH552 KeyTest`，诊断序列号 `08FC07BD-TEST`。该序列号是本次测试用固定标签，并非读取芯片 UID 生成。CDC 占接口 0/1、端点 0x81/0x82/0x02，键盘为接口 2、端点 0x83。键盘为 8 字节无 Report ID 报告。

## 触摸条怎么测试

```sh
rtk proxy uv run --no-project --with pyserial python firmware/ch552g-keytest/monitor.py
```

打开 [本地监测页](http://127.0.0.1:8766)。监测程序只自动打开匹配上述 VID/PID/序列号的测试设备，不打开其他串口；设备重插后自动重连。原始记录保存在 `captures/*.jsonl`，网页显示最近约 10 秒曲线。波特率设为 115200，CDC 并不依靠物理 UART 波特率传输。

1. **按键**：点击页面输入框，再逐个按键。每个物理键应只出现一个编号；页面应最终显示已测 17/17，松手后无键保持按下。使用英文/数字直接输入状态，避免输入法转换文本。
2. **静置**：手离开触摸条，点“松手后重新校准”，等校准完成，静置 5 秒观察各路噪声范围。
3. **分段**：从一端开始，每段停留约 1 秒，记录响应最强的通道次序。曲线 `Δ = baseline − raw`；典型触摸使原始计数下降、Δ 上升，但以实测为准。
4. **滑动**：往返慢滑，应看到相邻电极的响应连续交接。当前 C1～C5 只表示采样顺序，未标为物理坐标，也未启用鼠标或滚轮动作。
5. **长按**：按住 3 秒，差值仍应明显；松开后应回到静置噪声附近。固定基线会显示真实环境漂移，必要时松手重新校准。

先看曲线与静置噪声，再定阈值和电极空间顺序，最后做位置插值、点击/滑动手势。仅看到某一路数值变化不足以证明整条触控正常；长期为 0/65535、整段无响应或噪声与触摸幅度接近，都应检查接线和采样。

串口文本协议：

```text
K,millis,id,down
T,millis,frame_counter,stable_key_mask,raw_key_mask,calibrating,dropped,C1_raw,...,C5_raw,C1_base,...,C5_base
```

`down` 为 0/1；键位图 bit0～bit16 对应 ID 1～17。发送字符 `c` 可重新校准 RAM 基线。CDC 保留上游 1200 波特率关闭 DTR 跳 ISP 的入口，普通监测使用 115200，避免意外触发。

## 恢复官方程序

重新进入 ISP 后，将已保存的 [original.hex](../../refer/firmware/gai17touch-ver202/original.hex) 刷回：

```sh
rtk proxy wchisp flash refer/firmware/gai17touch-ver202/original.hex
```

此命令恢复用户提供的 Ver202 应用程序；该文件不是原设备完整 Flash 备份。EEPROM/配置保存结果应以当次 `captures/flash-*` 记录为准。

## 验证与来源

已通过 SDCC 编译，应用空间 6,730 字节 / 14,336 字节；动态分配的 XDATA 415 字节 / 758 字节，另预留 USB 区 266 字节。链接器提供约 191 字节栈空间，不代表实测最大栈深。上游 `wiring.c` 两处内联汇编返回导致 SDCC 的 return 提示，USB 空回调分支有 unreachable 提示；构建无错误。

`verify.py` 用电气扫描网络模拟执行实际 `main.c` 算法，覆盖全部 17 个键的编号、短脉冲消抖、长按不连发、松开、两位数字中重复数字、触摸校准/长按保留、CDC 背压；另独立解析 HEX 校验和、入口向量、应用区范围、USB 标识和调试协议。它们不能代替实物逐键和触摸检查。监测页的布局已在浏览器检查，实机调试串口为 `/dev/cu.usbmodem08FC07BD_TEST1`。最终固件 SHA-256 为 `048ceda9da5426be612edebfeed5f6b84627dd6c16d11b9ea6b17c4fa699c447`，在 15:39:58 完成 `Verify OK`。最后一次刷写证据为 `captures/flash-20260909-153955`；EEPROM 前后 SHA-256 同为 `6fc1549d8391fa3cd38cd39c7a899da6e0ebc0549f71f72024ea368520911e41`，配置比较的前 10 字节一致。运行日志见 `captures/20260909-153903.jsonl`。现场曾有一次写入中 USB 断开，重新进入 ISP 后已成功完整重刷并校验。

依赖 [ch55xduino](https://github.com/DeqingSun/ch55xduino/tree/c9f9a2a6516255284064a9dd248670545f25a322)，固定提交 `c9f9a2a6516255284064a9dd248670545f25a322`；SDCC 4.2.2 build 13407，下载包验证 SHA-256。`usb/` 基于该项目 `CdcHidCombo` 示例，修改 USB 字符串、键盘轮询间隔、HID 类请求和复位清理。上游许可证见 [LICENSE.ch55xduino](LICENSE.ch55xduino)。
