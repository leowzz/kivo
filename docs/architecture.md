# Kivo 的两个入口

Kivo 使用一个 Rust crate、两个独立的 Tauri 构建配置和两个前端入口。
默认构建运行 APP；启用 `product-studio` 时只装配 Studio。
这样可以共享产品格式、板卡约束和协议基础类型，同时让后台服务和应用数据各自独立。

| 边界 | APP | Studio |
|---|---|---|
| HTML | `index.html` | `studio.html` |
| React 入口 | `src/app/main.tsx` | `src/studio/main.tsx` |
| Rust 入口 | `src-tauri/src/app/mod.rs` | `src-tauri/src/studio.rs` |
| 应用标识 | `cn.wleo.kivo` | `cn.wleo.kivo.product-studio` |
| 前端构建目录 | `dist` | `dist-studio` |
| 开发端口 | 1420 | 1421 |
| 数据 | 设备登记、动作配置、旧版配置、统计与备份 | 产品仓库路径、仓库中的产品定义、构建产物 |
| 后台服务 | 设备发现、动作调度、粘贴、托盘、显示与用量 | 按需的产品构建和串口 GPIO 测试 |

Studio 的 Tauri window 明确加载 `studio.html`。不再把 Studio 页面复制成 APP 的
`index.html`，也不再通过 `client` / `embedded` 属性把整个 APP 塞进 Studio。
`make studio` 使用独立的 `src-tauri/target-studio` 开发编译目录，避免两个开发进程
争用同一个可执行文件。两个启动命令都不会自动终止另一入口。

## 职责

- `src/app/App.tsx`：主界面的快照、编辑草稿、自动保存、历史和设备设置流程。
- `src/app/DeviceManagement.tsx`：设备选择、动作配置选择、键盘与动作工作区；技术信息折叠展示。
- `src/studio/StudioRoot.tsx`：产品定义和工具台两个视图；切换时保留产品草稿，卸载串口测试视图。
- `src/studio/StudioApp.tsx`：产品定义编辑、校验、保存、复制、删除和固件构建。
- `src/studio/Toolbox.tsx`：工具台，负责设备选择、GPIO 采样、临时固件的输入模式和连接释放。
- `src/studio/FirmwareActions.tsx`：固件文件选择、备份和刷写确认、进度与结果展示。
- `src/studio/BuildResultPanel.tsx`：构建日志、输出路径复制和构建产物操作；`BuildFlashDialog.tsx` 负责目标设备选择和刷写确认。
- `src/studio/firmware.ts`：工具台和构建产物入口共用的固件 IPC、进度及错误格式。
- `src/shared/`：共享领域类型、颜色与基础控件样式，不持有运行服务或应用状态。
- `src/app/types.ts`：APP 独有的设备状态、动作配置和 IPC 快照；产品定义由共享类型统一维护。
- `src-tauri/src/app/mod.rs`：APP 启动、后台服务装配和关闭；`commands.rs` 负责 IPC 及工作区变更，`tests.rs` 验证行为。
- `src-tauri/src/studio.rs`：Studio 的仓库选择、产品文件操作和构建生命周期。
- `src-tauri/src/studio/gpio.rs`：独立串口会话、握手、采样解析和错误后的释放。
- `src-tauri/src/studio/firmware.rs`：固件操作 IPC、独占设备访问、进度通道与关闭保护。
- `scripts/studio_firmware.py`：固件文件校验、完整备份、临时固件构建和刷写；复用现有上传工具的物理设备定位。
- `src-tauri/src/{error,input,handshake,serial}.rs`：两个入口共用的错误、输入编码、握手和串口别名处理。
- `firmware/src/`：固件入口和平台适配；`lib/gpio_trigger/` 保留通用扫描、去抖和协议逻辑。

APP 不能修改产品固件中的布局或接线。产品配置只包含触发设置和按键动作，并按
Product Version ID 共享。改变产品定义后，需要在 Studio 构建并刷入新固件。

旧版 Device Profile 的运行、导入、分配和备份兼容路径继续保留，以免已有设备和
数据失效。逐键学习、旧版图形接线编辑器、旧版布局编辑器、未使用的仪表盘及其样式已移除。

## GPIO 诊断协议

Studio 先通过 USB 身份定位唯一设备，以 115200 波特率打开串口，再执行已有的
`HELLO` 握手校验。只有板卡支持的 GPIO 可以进入读数列表。

```text
GPIO_READ
GPIO_STATE 3 0:1 1:0 2:1
```

响应中的数字是引脚总数，后续每项为 `GPIO编号:电平`。实际响应包含该板卡的完整
safe pin 集合；Studio 拒绝缺失、重复、未知引脚和非 0/1 电平，按 GPIO 编号排序。
协议为行文本，单行最多 254 字节，响应超时为 2 秒。

这是新增的只读命令，不改变已有动作协议。旧固件无法响应时显示“当前固件不支持
GPIO 测试”。固件不改变引脚模式、上下拉、输出电平或运行拓扑，也不创建学习草稿。

每次读取完成后等待 125 ms 再次读取，不会叠加并发请求。会话 ID 隔离过期请求；
读取失败清空会话和界面读数。停止、离开视图、关闭窗口都会释放串口。
连接未完成就离开页面时，前端会等待连接结束再释放对应会话。

GPIO 读数是瞬时数字采样。悬空输入、矩阵行扫描和 I2C 引脚的瞬时状态并不等于
导线连通性；结合已知接线和按下/释放时预期的电平变化进行检查。

## 固件操作

固件操作只属于 Studio，不依赖 APP 的设备运行服务。前端锁定所选设备及工作区，
后端使用同一个 GPIO 会话锁释放串口并排除并发采样。固件操作结束前拒绝窗口关闭，
避免因关闭应用而中断 Flash 写入。进度通过 Tauri Channel 返回。

备份使用 picotool `save -a` 或 esptool `read_flash 0 ALL` 读取整片 Flash，先写入
目标目录下的临时文件，读取完成后原子替换保存路径，再尝试重启设备。
自动重启失败不会把已保存的备份误报为丢失。

文件刷写检查 RP2040 UF2 的块结构、Flash 地址和芯片 family ID，或 ESP32-S3
完整 Flash / factory BIN 的芯片头，拒绝应写入应用分区的独立 BIN。产品清单和
备份校验记录若存在也会核对；确认后的 SHA-256 在暂存时再次检查，避免并发构建
替换源文件。可以明确选择另一个产品固件，也可以通过文件入口恢复原固件备份。
RP2040 按 Flash ID 定位，ESP32-S3 在下载模式再核对芯片 MAC；失败后的同视图重试
可以定位留在引导模式的原设备。刷写完成后校验 HELLO 的板型、构建和产品身份，
普通镜像验证 Flash 内容并等待运行时 USB；临时测试固件验证握手后自动开始采样。
工具子进程有超时；测试使用模拟设备，不会自动刷写实体硬件。

## 临时 I/O 固件

`firmware/src/io_test.cpp` 是独立的串口入口，PlatformIO 的 `io-test-rp2040` 和
`io-test-esp32s3` 环境只编译此入口，不链接产品接线、动作控制、矩阵扫描和显示驱动。
`IoTestProtocol.h` 只接受 `HELLO`、`GPIO_READ` 和 `GPIO_MODE [INPUT|PULLUP|PULLDOWN]`。
模式改变只操作板卡允许的输入引脚，没有输出驱动命令；上电与串口断开后使用内部上拉输入，
使开路触点具有确定的高电平，避免悬空噪声。Studio 每次建立临时固件会话时先确认
`GPIO_MODE PULLUP`，再开始采样，因此默认浮空的旧测试固件无需重新刷写也能使用。
手动切换浮空或下拉只影响当前测试会话，轮询不会覆盖手动选择。读数仍是原始电平，
不通过平滑掩盖跳变。上拉输入接地时读低；两个上拉输入间的矩阵触点需要另行扫描。
普通产品固件仍只读电平，输入模式命令限定于构建 ID 以 `io-test-` 开头的临时固件。

构建隔离在 `.pio/io-test/`，产物写入 `output/io-test/<board>/<build-id>/`，不依赖
Product Definition。首次安装前保存整片 Flash 及设备/校验记录，再原子写入设备对应的
`active.json`，此后才允许覆盖固件。失败重试或重复安装复用原备份；备份丢失、损坏，
或设备已是测试固件但没有原备份时，阻止重新覆盖。恢复或刷入其他固件成功后清理活动
记录，保留备份文件；重新打开工具台仍能从仓库读取尚未恢复的原备份路径。

## 验证

`make test` 覆盖 APP 和 Studio 的 Rust 测试、Clippy、前端交互测试、Studio
打包隔离检查、固件 native 测试，以及发布、上传和串口工具的 Python 测试。
Studio 产物不得包含 APP 的运行分配、配置保存或运行事件桥接代码。

真实固件构建使用 `make build-esp32s3 build-rp2040`。自动测试验证协议和生命周期，
物理电平、接线与 HID 输出仍需要目标板卡验收。
