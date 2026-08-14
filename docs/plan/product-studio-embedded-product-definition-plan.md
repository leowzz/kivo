# Kivo Product Studio 与内嵌产品定义实施计划

> 状态：Implemented (2026-08-14)
>
> 本文记录当前已确认的实现方向。后续实现应以本文为准，并保留现有 DIY
> Device Profile 与 Runtime Assignment 工作流的兼容性。

## 1. Summary

- 新增仅通过 `make studio` 启动的 Product Studio Tauri 开发 flavor；普通
  Kivo 安装包不包含 Studio 页面、YAML 写入或构建命令。
- Product Definition 作为唯一生产源文件，包含产品身份、按键布局和硬件拓扑，
  不包含用户 Actions。
- 生产固件同时嵌入编译后的运行拓扑与完整语义定义；普通 Kivo 通过协议读取定义，
  实现免配置识别。
- 用户 Actions 按 Device ID 独立保存；现有 DIY Device Profile 与 Runtime
  Assignment 保持兼容。
- 首版完成编辑、校验、构建闭环，不包含 Studio 内刷机和实体试机。

## 2. Product Contract And Build

- 产品源文件固定为
  `products/<product-version-id>/product.yaml`，使用
  `ProductDefinition schema_version: 1`：
  - `product`：display name、family ID、variant ID、hardware revision、
    Product Version ID、capability tokens。
  - `layout`：复用现有 `ModelLayout`。
  - `hardware_profile`：复用现有 Hardware Profile 数据结构，但每个 Product
    Version 只能选择一个确定的硬件实现。
  - 明确禁止 `actions` 字段；长按和双击参数属于用户 Action 配置。
- Rust 新增统一的 Product Definition 解析、序列化和校验模块，Studio、构建
  CLI、桌面端设备读取共用同一实现。
- 校验 Product Version ID 组成、布局按键数量、Board Profile、GPIO 白名单、
  引脚冲突、矩阵二分图、功能开关目标和 SSD1306 能力。
- 当前支持直连键、接点矩阵、功能开关、SSD1306，以及作为显示组件配置的
  `ec11_confirm_back` 控制面板。控制面板统一配置确认、编码器按压、编码器 A/B 相和
  返回五路 GPIO，不把它们加入按键布局；`mic` 等其他 token 仍只记录产品能力。
- YAML 经验证后转为确定性 JSON，限制为 64 KiB，计算 SHA-256；固件嵌入该
  JSON，同时由相同对象生成编译期 topology header。
- 新增共享构建服务及 `kivo-product` CLI；
  `make build-product PRODUCT=<product-version-id>` 与 Studio 调用同一服务，
  不重复实现校验或代码生成。
- 生成目录使用 `.pio/product/<sha256>/`，发布输出为
  `output/products/<product-id>/<build-id>/`，包含固件、规范化定义和
  `manifest.json`。
- `manifest.json` 记录 Product Version ID、协议版本、Product Definition 摘要、
  Board Profile、Git commit、固件大小和固件 SHA-256。
- Product Studio 同时只允许一个构建任务；重复构建返回 busy，关闭 Studio 时
  终止其子进程。首版不生成签名、不修改 `make release`。

## 3. Studio And Runtime

- 增加 `studio.html`、独立 React 入口和 `tauri.studio.conf.json`；Cargo feature
  `product-studio` 注册 Studio 专属命令并切换到无托盘、关闭即退出的 Studio
  runtime。
- `make studio` 先执行 `helper-kill`，设置经过 canonicalize 的仓库根目录，再使用
  `tauri dev --features product-studio --config ...` 启动。
- 所有 Studio 文件操作限制在当前仓库的 `products/` 和 `output/products/`。
- Studio 提供产品列表、新建或复制版本、产品身份、布局、硬件引脚、规范化定义预览、
  校验结果和构建日志。
- 已保存 Product Version ID 不允许原地重命名；新修订通过复制创建。
- 编辑采用内存 draft 和显式保存；存在校验错误或未保存修改时禁止构建。
- 保存复用原子临时文件替换，不提供删除入口。
- 固件协议 v9 为 `HELLO` 增加 `<product-version-id|->`；v10 增加
  `CONFIG_OLED_CONTROL`，用于原子配置显示控制面板的五路 GPIO。旧协议 3-8 和 `-`
  继续进入 legacy 流程，v9 产品固件继续支持不带控制面板的定义。
- 新增 `PRODUCT_INFO` 与 `PRODUCT_READ` 请求，以及 `PRODUCT_BEGIN`、顺序编号的
  `PRODUCT_CHUNK`、`PRODUCT_END` 响应。
- 每块原始数据最多 144 字节；Host 严格验证顺序、64 KiB 上限、15 秒超时、长度和
  SHA-256。
- 固件启动时立即激活生成的 topology；Host 读取并验证定义后，仍使用现有
  `CONFIG_*` 事务下发同一 topology，以保留当前 `CONFIG_OK`、学习和临时调试机制。
- 固件重启后始终恢复内嵌 Product Definition。
- 桌面端按摘要原子缓存规范化定义；缓存损坏、摘要错误、schema 不支持或 Product ID
  不一致时重新读取或将 Device 标记为需处理，不能静默降级到错误型号。

## 4. User Data And Compatibility

- Settings schema 升级，Device Record 增加可选 `ProductDeviceConfig`：Product
  Version ID、每设备 Trigger Settings 和按键 Actions。
- 默认长按和双击阈值保持 500 ms 与 300 ms，默认 Actions 为空。
- Product Device 连接后自动登记并显示内嵌布局，不创建 Runtime Assignment，也不进入
  按键布局或硬件配置向导。
- 保存 Actions 时，后端必须按当前 Product Definition 校验按键 ID。
- Runtime 层增加统一的 resolved configuration：Product Device 使用内嵌
  layout/hardware 与每设备 Actions；legacy Device 继续使用 Device Profile、
  Hardware Profile 和 Runtime Assignment。
- 同型号的不同 Device 各自保存 Actions；提供显式“从另一设备复制”，但不存在共享可变
  Action Profile。
- Product Device 上已有 legacy Runtime Assignment 时保留但不启用；重新刷回无 Product
  Definition 的通用固件后仍可恢复 legacy 行为。
- 默认用户备份改为轻量 schema，仅包含 Device ID、Product Version ID、Trigger
  Settings 和 Actions。
- 轻量备份恢复按 Device ID 合并，未列出的设备不变，Product Version ID 不匹配时不应用。
- 现有全量备份保留只读预览和导入兼容，但不再作为普通用户默认导出。
- Product Definition 缓存、硬件布局和指标不进入轻量备份。
- 重写现有 Future 后台设计：未来固件发布以不可变 Product Definition 和产品专用固件为
  单位，不再采用“通用固件 + 独立 Configuration Release”。
- 用户 Actions 仍为本地数据；远程 Action 管理不属于本计划。

## 5. Test Plan

- Rust：Product Definition YAML/JSON round trip、所有身份和硬件校验、确定性摘要、
  大小限制、原子保存、生成 header 与 manifest。
- Firmware native：内嵌 topology 启动、无产品 legacy 启动、v10 HELLO、定义分块、
  255 字节行限制、错误请求和重启恢复。
- Host protocol：协议 3-8 回归、v9 产品定义读取、v10 显示控制面板配置、缓存命中、
  缺块、乱序、base64、长度、SHA、超时和 schema 错误。
- Runtime integration：Product Device 零配置进入可编辑状态、同型号两台设备 Actions
  隔离、动作执行、定义变化导致未知按键错误、legacy Runtime Assignment 不回归。
- Backup：轻量导出与合并恢复、离线 Device 恢复、型号不匹配、旧全量备份导入兼容。
- Studio UI：新建或复制、布局和引脚编辑、校验阻止保存或构建、dirty 状态、构建成功、
  构建失败和 busy 日志。
- Packaging：普通 `npm run build` 和 `make helper-build` 不包含 Studio 前端、Studio
  commands 或 `products/`。
- Makefile：验证 `make -n studio` 和 `make -n build-product` 的命令与参数。
- 最终运行 `direnv exec . make test`、目标 Product 的实际固件构建和
  `git diff --check`。
- 刷机与实体硬件验收不属于首版自动验收，必须明确记录为 Not Run。

## 6. Assumptions

- Source YAML 的语义完整嵌入固件，但注释和原始排版不保留；设备传输的是规范化 JSON。
- Product Version ID 在首次保存后不可变；同一 ID 下定义摘要变化仅用于开发阶段，生产
  变更必须创建新的 Hardware Revision。
- 普通用户可以修改 Actions 和触发阈值，不能修改内嵌布局或硬件引脚。
- 实施时保留当前工作树中的 UX、命名文档及其他未提交修改，不覆盖或回退无关内容。
