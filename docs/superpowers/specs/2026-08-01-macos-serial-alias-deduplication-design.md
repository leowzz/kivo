# macOS 串口别名去重设计

日期：2026-08-01

## 背景

macOS 为同一个串口服务同时创建 callout 节点 `/dev/cu.<name>` 和
dial-in 节点 `/dev/tty.<name>`。Kivo 当前使用的 `serialport 4.9.0` 会把
两者作为两个 `SerialPortInfo` 返回，并为两者附上相同的 USB VID、PID 和
硬件序列号。

`SystemUsbEnumerator` 当前把每个 `SerialPortInfo` 原样转换为一个
`SerialObservation`。Coordinator 随后按 Board Profile 和硬件序列号生成
Device ID；同一 Device ID 对应多个 observation 时会进入
`duplicate_identity` 隔离。因此，一个物理 RP2040 会被错误显示成两个待处理
设备，分别对应 `/dev/cu.*` 和 `/dev/tty.*`。

这不是固件问题，也不是两个设备使用了重复序列号。实机上的
`/dev/cu.usbmodem2101` 与 `/dev/tty.usbmodem2101` 属于同一个
`IOSerialBSDClient`。

## 目标

- 同一个 macOS 串口服务的成对 `/dev/cu.<name>` 与 `/dev/tty.<name>` 只产生
  一个 runtime observation。
- 成对节点优先保留 `/dev/cu.<name>`，供 Kivo 主动打开串口。
- 如果只有 `/dev/tty.<name>` 而没有对应 `/dev/cu.<name>`，仍保留该节点。
- Windows、Linux 和不符合 macOS 配对命名规则的端口保持现有行为。
- 两个真实设备声明相同硬件身份时，继续进入 `duplicate_identity` 隔离。

## 非目标

- 不按硬件序列号直接去重。
- 不改变 Device ID、Candidate、HELLO v3 或 Runtime Assignment 规则。
- 不修改前端设备列表或技术详情。
- 不构建、刷写或修复 RP2040 固件。
- 不尝试打开 `/dev/tty.*` 与 `/dev/cu.*` 来判断它们是否为同一设备。

## 选定方案

在 `SystemUsbEnumerator` 的系统端口结果进入 `SerialObservation` 转换前，增加
一个纯数据归一化步骤。

归一化先收集所有 `/dev/cu.<suffix>` 的 suffix，然后遍历原始端口：

- `/dev/tty.<suffix>` 存在同 suffix 的 `/dev/cu.<suffix>` 时，丢弃 tty 节点。
- `/dev/tty.<suffix>` 没有配对 cu 节点时，保留 tty 节点。
- `/dev/cu.*`、Windows `COM*`、Linux `/dev/ttyACM*`、`/dev/ttyUSB*` 和其他
  名称全部保留。

该规则依据系统节点的明确配对关系去重，而不是依据 VID、PID 或硬件序列号。
因此两块真实设备即使错误地声明同一个序列号，只要它们有不同的 callout
节点，就仍会产生两个 observation，并由现有 Coordinator 冲突规则隔离。

## 组件边界与数据流

1. `serialport::available_ports()` 返回原始 `Vec<SerialPortInfo>`。
2. 新的私有归一化函数折叠成对的 macOS dial-in/callout 别名。
3. `SystemUsbEnumerator` 继续只把 USB 端口转换为 `SerialObservation`。
4. Board Profile 分类、Device ID 分组和 Coordinator reconcile 保持不变。
5. 对当前 RP2040，Coordinator 只收到
   `/dev/cu.usbmodem2101`，因此不会误入 `duplicate_identity`，而会启动现有
   worker 并进入 HELLO/固件协议检测。

归一化函数放在系统枚举边界，不放在 reconcile 中。reconcile 接收的
observation 仍代表独立系统观察结果，并继续严格处理真实身份冲突。

## 错误与边界条件

- 只有 tty：保留，避免设备完全消失。
- 只有 cu：保留。
- 同 suffix 的 tty/cu 顺序任意：始终只保留 cu。
- 多组 tty/cu：每组独立折叠。
- 两个不同 cu 节点共享序列号：不合并，继续报身份冲突。
- 非 USB 串口：仍由现有 USB 类型过滤排除；本设计不改变该规则。
- 枚举失败：继续返回现有 `serialport` 错误，不吞掉或改写错误。

## 测试设计

### 单元测试

- 输入 `/dev/cu.usbmodem2101` 与 `/dev/tty.usbmodem2101`，输出仅包含 cu。
- 交换输入顺序，输出仍仅包含 cu。
- 仅输入 `/dev/tty.usbmodem2101`，tty 被保留。
- 输入 `COM4`、`/dev/ttyACM0` 等非配对名称，全部保留。
- 输入两个不同 `/dev/cu.*` 且 USB 序列号相同，两者都保留。

### Coordinator 回归

- 现有多个 observation 共享同一 Device ID 的测试继续通过，证明真实重复身份
  检测没有被削弱。
- 系统枚举转换测试验证配对别名在进入 Coordinator 前已折叠。

### 完整验证

- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `npm test`
- `npm run build`
- `git diff --check`

## 实机验收

保持 RP2040 连接并重新启动包含修复的 Kivo：

1. 顶部只统计一个待处理或就绪设备，不再因 tty/cu 显示两个。
2. 设备列表只出现一个序列号末尾为 `4E811C` 的条目。
3. 技术详情中的系统通信端口为 `/dev/cu.usbmodem2101`。
4. 不再出现由 tty/cu 别名导致的“设备身份冲突”。
5. 若固件未通过 HELLO v3，应用进入现有固件问题提示；该提示与串口别名修复
   相互独立。

实机验收不执行硬件刷写。若固件不兼容，只记录并展示现有固件问题。

## 成功标准

一个物理 RP2040 在 macOS 上只形成一个 Kivo runtime observation。配对串口
别名不会触发身份冲突，而真实的重复硬件身份仍被隔离。
