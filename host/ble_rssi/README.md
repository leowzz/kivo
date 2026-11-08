# BLE RSSI 历史曲线

从 ESP32-S3 的 USB 串口采集 RSSI，在本机网页显示实时信号、历史曲线和位置标记。手机使用系统蓝牙连接，无需额外程序；先按[固件说明](../../firmware/demos/ble-rssi/README.md)烧录并配对。

## 启动

在仓库根目录运行：

```sh
uv sync --group dev
uv run python -m host.ble_rssi.viewer
```

打开 <http://127.0.0.1:8765>。服务只监听本机，默认选择唯一的 `303a:1001` USB Serial/JTAG 设备。采集程序不会主动复位开发板；启动前关闭占用目标串口的其他监视器。

连接多台开发板时，先用 `uv run pio device list` 查看 USB 序列号，再显式选择：

```sh
uv run python -m host.ble_rssi.viewer --serial ABCDEF123456
```

`--serial` 接受带冒号或不带冒号的 USB 序列号，忽略大小写。也可以使用 `--serial-port /dev/cu.usbmodemXXXX` 直接指定串口；它优先于自动选择。端口或设备暂时消失后，采集程序会继续尝试连接。这里选择的是开发板的 USB 身份，与手机蓝牙 MAC 无关。

## 使用与保存

- 曲线显示原始 RSSI 和 EMA 平滑值（系数 0.2），支持最近 1、5、15 分钟或全部历史。
- 在手机所在位置添加标记，便于回看或导出后对照。网页显示北京时间，CSV 时间使用 UTC 并包含时区。
- “暂停查看”只冻结当前视图，后台继续采集和保存。失去连接或采样间隔超过 3.5 秒时，曲线分段；旧读数不会显示成当前信号。
- 默认历史文件是仓库内 `output/ble-rssi/history.jsonl`。重启时读取已有记录，跳过损坏行，并继续追加；不要在服务运行时编辑文件。
- 服务内存最多保留最近 86,400 条记录，包含采样、连接事件和标记。CSV 导出这部分记录；JSONL 文件保留全部追加历史。

可指定历史文件和 HTTP 端口：

```sh
uv run python -m host.ble_rssi.viewer \
  --http-port 8765 \
  --data-file output/ble-rssi/desk-test.jsonl
```

按 `Ctrl+C` 停止。继续同一次实验时使用同一个 `--data-file`；开始独立实验时指定新文件。`output/` 已被 Git 忽略。

## 源码与检查

```text
host/ble_rssi/
  viewer.py        串口采集、JSONL 历史、HTTP API 与静态资源服务
  web/
    index.html     页面结构
    app.js         数据轮询、曲线与交互
    styles.css     页面样式
test/
  test_ble_rssi_viewer.py
```

网页使用原生 HTML、CSS 和 JavaScript，无需前端构建或 CDN。相关检查在仓库根目录运行：

```sh
uv run pytest test/test_ble_rssi_viewer.py
node --check host/ble_rssi/web/app.js
```

API 保持简单：`GET /api/state?after=<id>` 返回增量记录和当前状态，`POST /api/marker` 接收 `{"label":"位置"}`，`GET /api/export.csv` 导出历史。`/viewer.html` 保留为首页别名。

RSSI 的阈值限制与补测方法见[实验记录](../../docs/research/ble-rssi-presence.md)。本工具尚未实现离开、回来判定或屏幕控制。
