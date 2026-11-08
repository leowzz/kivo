# BLE RSSI 固件实验

ESP32-S3 N16R8 的独立 PlatformIO 工程，使用 Arduino ESP32 2.0.17，输出 115200 波特率的原生 USB Serial/JTAG 日志。

- `src/connected_rssi.cpp`：手机通过系统蓝牙配对后，读取连接 RSSI。默认构建目标。
- `src/scan_rssi.cpp`：按 MAC 扫描 BLE 广播，附带可选名称发现模式。
- 电脑端采集与实时曲线：[host/ble_rssi](../../../host/ble_rssi/README.md)。
- 实测数据与阈值限制：[实验记录](../../../docs/research/ble-rssi-presence.md)。

## 准备

在仓库根目录安装开发依赖，并找到目标开发板串口：

```sh
uv sync --group dev
uv run pio device list
```

下方 `ESP32_PORT` 是示例，按实际 ESP32-S3 端口替换。烧录会替换目标板固件，操作前保留原固件备份，并确认没有选中其他 Kivo 设备。串口监视器和网页采集服务需要独占该端口，烧录前先退出。

## 配对后读取 RSSI

```sh
ESP32_PORT=/dev/cu.usbmodem31201
uv run pio run -d firmware/demos/ble-rssi -e ble-connected-rssi
uv run pio run -d firmware/demos/ble-rssi -e ble-connected-rssi -t upload --upload-port "$ESP32_PORT"
uv run pio device monitor -d firmware/demos/ble-rssi -e ble-connected-rssi --port "$ESP32_PORT"
```

在手机系统蓝牙设置中选择 `Kivo RSSI Demo` 并配对，无需安装手机程序。固件使用 BLE HID Consumer Control 协议，不发送按键事件。连接后每秒读取实际连接对端的 RSSI：

```text
LINK state=CONNECTED peer=AA:BB:CC:DD:EE:FF
RSSI t_ms=12345 peer=AA:BB:CC:DD:EE:FF rssi_dbm=-62 source=connection
```

数值越接近零，信号越强。这里的 `peer` 来自连接回调，可能不同于手机设置中的蓝牙地址。`RSSI_UNAVAILABLE`、`RSSI_REQUEST_FAILED`、`RSSI_TIMEOUT` 表示没有得到有效新读数；错误或无效值 `127` 不会作为 RSSI 输出。断开后固件重新广播，等待手机重连。

HID 服务必须先启动，再调用会发送通知的 `setBatteryLevel()`；Arduino ESP32 2.0.17 中顺序颠倒会在启动时崩溃。

## 扫描广播

先在 `src/scan_rssi.cpp` 中填写 `kTargetMac`，格式为 `AA:BB:CC:DD:EE:FF`，再构建：

```sh
uv run pio run -d firmware/demos/ble-rssi -e ble-rssi
uv run pio run -d firmware/demos/ble-rssi -e ble-rssi -t upload --upload-port "$ESP32_PORT"
```

`ble-rssi` 未配置有效地址时会输出 `TARGET_NOT_CONFIGURED`。目标必须使用指定地址发送可接收的 BLE 广播或扫描响应，广播里可以没有名称。

`ble-discovery` 环境另外输出名称包含 `kCandidateName` 的候选设备，用于寻找地址线索；无需预先填写目标 MAC，候选不会更新目标计数。默认名称关键词为 `xiaomi`，可按目标修改。

```sh
uv run pio run -d firmware/demos/ble-rssi -e ble-discovery
```

手机系统蓝牙名称和系统 MAC 不保证出现在 BLE 广播中，扫描不到不能直接判定手机或人离开。[Android 广播名称 API](https://developer.android.com/reference/android/bluetooth/le/AdvertiseData.Builder#setIncludeDeviceName(boolean))

三个环境均只采样，不控制屏幕，也不包含离开、回来阈值判断。
