# Kivo 面向大众用户的 UX 优化方案

- 日期：2026-08-14
- 状态：草案，待评审
- 适用范围：`src/`（React 界面与文案）、`src-tauri/`（一键刷机、签名等 Rust 侧改动）、`docs/`（README 与帮助文案）
- 目标用户：购买成品键盘（Keypad / 电话改造机）的普通消费者，不接触开发板、GPIO、固件等概念

## 1. 背景与目标

Kivo 当前把一套硬件调试工具的领域模型（Device Profile / Hardware Profile /
Runtime Assignment / Board Profile / GPIO / 消抖 / 固件协议）直接暴露给了用户。
README 的"快速上手"是 6 步，其中刷固件、新建 Hardware Profile、保存 Runtime
Assignment 三步是纯技术操作；Windows 安装包未签名，首次运行有 SmartScreen 风险。

**目标**：把"插上就能用、点按键就能配"作为默认体验，技术概念全部收进"高级"。
参考标杆是 Elgato Stream Deck：插上 → 点按键 → 设动作 → 完事。

**非目标**（本方案不做）：

- 不改变硬件接线能力（直连 GPIO / 接触矩阵 / 功能开关 / OLED 仍保留）。
- 不改变领域数据模型（Device Profile、Hardware Profile 仍存在于存储与协议层）。
- 不引入账号体系、云同步。
- 不改变固件协议。

## 2. 现状诊断

### 2.1 首次上手路径过长（P0）

当前完整路径（来自 README 与 `src/DeviceSetupWizard.tsx`）：

1. 下载安装包（Windows 未签名 → 可能的 SmartScreen 拦截）。
2. 刷入固件：RP2040 走 BOOTSEL + 拖拽 UF2；ESP32-S3 走浏览器 esptool。两个都是技术操作。
3. 连接设备，等待自动登记。
4. 新建 Device Profile（`CreateDeviceProfileForm`）。
5. 新建 Hardware Profile，手动配 GPIO 或进入"逐键学习"（`HardwareMapping`）。
6. 在设备管理中保存 Runtime Assignment（`DeviceManagement`）。

向导（`DeviceSetupWizard.tsx`）中普通用户必须面对两个下拉框：
`setup.deviceProfile`（键盘配置）与 `setup.hardwareProfile`（硬件配置）——
绝大多数消费者不理解为什么需要选两个东西。

### 2.2 术语是开发者语言（P0）

`src/i18n.ts` 全中文，但词都是技术词："设备配置"、"硬件配置"、"运行分配"、
"板型"、"控制器系列"、"原始序列号"、"固件构建"、"消抖"、"矩阵引脚"。
顶栏状态"就绪 / 需处理 / 离线"是运维语言；设备详情默认展示诊断字段
（`devices.serial`、`devices.id`、`devices.controller`、`devices.firmware`、
`devices.pins`、`devices.error`），没有折叠。

### 2.3 信息架构是开发者视角（P1）

导航：首页 / 设备管理 / 按键行为 / **配置文件**。
"配置文件"页（`App.tsx` 的 `view === "data"`）默认展示导入 / 导出 / 备份 / 恢复 /
复制 / 删除 YAML——这是开发者功能，对消费者应整体降级。

### 2.4 动作编辑器功能密度过高（P1）

`ActionEditor.tsx` 本身交互良好（可视化按键图 + 动作列表 + 快捷键录入），
但触发方式（按下 / 释放 / 长按 / 双击）与毫秒级阈值默认全量暴露，
对普通用户是噪音。

### 2.5 已有亮点（保留并强化）

- 自动保存（`useAutosave`），无保存焦虑。
- 逐键学习模式（`hardware.learn`）——"按一下键，它认出自己"，这恰恰是面向消费者的杀手功能，当前却被埋在"适配新设备"高级入口里。
- 按键按下时界面高亮（`pressedButtonIds`）与活动日志——即时反馈闭环。
- 首页指标（今日按下 / 活跃按键 / 热力图）——成就感设计。
- candidate 错误卡片文案已较友好（`candidate.*`）。
- 中文界面。

## 3. 目标心智模型

用户只接触三个对象：

| 用户视角对象 | 对应现有概念 | 用户理解 |
|---|---|---|
| 键盘 | Device（设备） | 我买的这个东西 |
| 键盘上的按键 | Device Profile 中的按钮 | 一个可以按的键 |
| 按键动作 | Button Action（粘贴 / 快捷键 / 打开 / 媒体） | 按下去会发生什么 |

隐藏项（全部自动完成或收进高级）：Hardware Profile（自动按板卡选择）、
Runtime Assignment（自动生效）、Board Profile / GPIO / 引脚 / 消抖 / 固件（诊断区）。

## 4. 分阶段方案

### 阶段 1（1–2 周，纯前端 + 文案，不动固件）

目标：用户感知最大提升、风险最低。全部改动集中在 `src/`。

#### 4.1 术语替换表

以下替换 `src/i18n.ts` 中的文案值（key 结构不变，`t()` 调用点只需在视图层同步微调）。

| i18n key（现状） | 现状文案 | 建议文案 |
|---|---|---|
| `nav.devices` | 设备管理 | 我的键盘 |
| `nav.behavior` | 按键行为 | 按键设置 |
| `nav.data` | 配置文件 | 数据与备份（高级） |
| `device.ready` | 就绪 | 已连接 |
| `device.attention` | 需处理 | 需要设置 |
| `device.offline` | 离线 | 未连接 |
| `device.runtimeAssignment` | 运行分配 | 正在使用 |
| `device.boardProfile` | 板型 | 板卡型号 |
| `model.label` | 设备配置 | 键盘设置 |
| `setup.deviceProfile` | 键盘配置 | 键盘设置 |
| `setup.hardwareProfile` | 硬件配置 | 接线方案 |
| `hardware.profile` | 硬件配置 | 接线方案 |
| `hardware.advanced` | 适配新设备 | 自动识别按键 |
| `hardware.learn` | 逐键学习 | 自动识别按键 |
| `hardware.direct` / `matrix` | 直连 GPIO / 接触矩阵 | 收进高级，仅保留接线方案名称 |
| `devices.workspaceIo` | I/O 映射 | 接线（高级） |
| `devices.workspaceLayout` | 按键布局 | 按键布局（保留） |
| `behavior.trigger.*` | 按下/释放/长按/双击 | 收进"高级触发方式" |

同时清理面向普通用户的错误与状态文案：`candidate.firmware_not_responding`、
`candidate.firmware_incompatible`、`candidate.bootloader` 等卡片在默认视图只显示
"设备需要更新，点击自动修复（阶段 3 前：请按说明书操作）"，技术细节进折叠区
（复用现有 `setup.technicalDetails` 折叠模式）。

#### 4.2 导航与信息架构

`src/App.tsx` 的侧栏调整为：

```
首页          （Home）
我的键盘      （DeviceManagement，名称替换）
按键设置      （Behavior）
数据与备份    （折叠为次级入口，或移入"设置"弹窗）
```

具体改动：

- 侧栏去掉 `nav.configuration` 分组层级，三个主项平级。
- 顶栏状态文案按 4.1 替换；`attentionCount` 的含义改为"需要设置"。
- `view === "data"` 页整体降级：保留导入/导出/备份/恢复功能，但入口从主导航移入
  "设置"弹窗的"高级"区；列表中的 `profile.profile.id` 等 `code` 字段改为折叠显示。

#### 4.3 设备详情折叠技术字段

`src/DeviceManagement.tsx` 中 Diagnostics 区（原始序列号、设备 ID、控制器系列、
固件构建、上报引脚、最新错误）整体包进 `<details>`，默认收起；
"运行分配 / 板型 / 端口"等字段只在设备需要修复时展开显示。

#### 4.4 动作编辑器默认简化

`src/ActionEditor.tsx`：

- 触发方式（按下/释放/长按/双击）默认折叠进"高级触发方式"，默认只显示"按下时"。
- 长按/双击阈值（`ConfigurationSettingsDialog` 的毫秒输入）改为人性化输入
  （"长按 0.5 秒"），或整体移入高级设置。
- "等待"动作的毫秒输入增加秒/毫秒切换。
- 动作类型保持五种不变（粘贴文本 / 快捷键 / 打开目标 / 媒体 / 等待）。

#### 4.5 空状态与引导

- 首页（`HomeDashboard.tsx`）无设备时，主按钮为"插上你的键盘"，不再是"新建配置"。
- 无任何 Device Profile 时，引导文案指向"从模板开始"（见 5.2）。
- 所有空状态遵循"下一步该做什么"句式，不展示错误码。

### 阶段 2（2–4 周，前端 + 少量 Rust）

目标：让接线与分配对用户完全隐形。

#### 5.1 向导重构（`src/DeviceSetupWizard.tsx`）

新流程（三步）：

1. 插上键盘 → 自动识别，弹出"欢迎！给键盘起个名字"（默认名 = 设备名）。
2. 选择预设模板或"从空白开始"（替代"新建 Device Profile"）。
3. 完成 → 自动完成 Runtime Assignment（自动选择兼容的 Hardware Profile）。

改动要点：

- 删除 `setup-profile-choice` 阶段的两个下拉框（`setup.deviceProfile` /
  `setup.hardwareProfile`）：自动选择唯一兼容 Hardware Profile；若有多个，
  默认取第一个并在确认页以"接线方案"名称展示，可展开更换。
- 保留 `setup-confirmation` 阶段的名称输入与设备信息摘要，去掉技术字段。

#### 5.2 预置模板（`models/prod/` 已有资产，包装成用户可见选择）

随 app 提供 3–5 个开箱即用模板，如：

- 直播快捷面板（常见话术粘贴 + 媒体控制）。
- 视频剪辑台（撤销/复制/粘贴/分割快捷键）。
- 电话拨号盘（数字 + 回车 + 常用号码粘贴）。

模板选择在新建键盘与首页空状态两个入口都出现。模板本质仍是 Device Profile，
无需新数据模型。

#### 5.3 学习模式提升为默认接线入口

`HardwareMapping.tsx` 的"逐键学习"从"高级/适配新设备"提升为新建接线方案时的
主选项："按一下键盘上的键，它会自己认出自己"。手动 GPIO 配置保留在高级区。

#### 5.4 一键刷机（需要 Rust + 固件侧配合）

目标：消除 BOOTSEL 拖拽与 esptool 两条技术路径。

- 检测到未刷固件 / 固件不匹配的板卡时，界面显示"需要安装键盘驱动，点击继续"。
- App 自动识别板卡 → 下载匹配固件（与 Release 资产同源）→ 自动进入
  bootloader（RP2040 自动复位进 BOOTSEL；ESP32-S3 走内置 ROM 下载模式）→
  显示进度条 → 完成后自动登记设备。
- 失败路径回落到现有"按说明书操作"指引与诊断折叠区。
- 同时保留 `scripts/` 的开发者上传流程（`make upload-*`），两者不冲突。

#### 5.5 共享配置提示简化

`devices.sharedWarning`（"被 N 个设备使用"）仍保留，但文案改为
"这个设置也被其他键盘使用，改动会影响它们"，并提供"复制一份仅用于这台键盘"。

### 阶段 3（2–4 周，分发与信任）

- **Windows 代码签名**：消除 SmartScreen 拦截，这是大众化最关键的信任门槛。
- 系统托盘常驻、开机自启的说明用通俗文案（"Kivo 在后台运行，按键随时可用"）。
- 首页/设备详情增加"按键按下实时高亮 + 日志"的强化（基于现有
  `pressedButtonIds` 与 `home.logs`），确保无 OLED 设备也有即时反馈。
- README 重写"快速上手"为消费者版本（3 步），技术内容移入"开发者文档"章节。

## 6. 反馈与状态设计

| 状态 | 现状文案 | 建议 | 行为 |
|---|---|---|---|
| 已连接且分配有效 | 就绪 | 已连接 | 绿色，无操作项 |
| 已连接但未分配/有候选 | 需处理 | 需要设置 | 琥珀色，点击直达向导 |
| 断开 | 离线 | 未连接 | 灰色 |
| 身份冲突/无效 | 设备身份冲突 | 需要处理 | 展开修复指引 |

首页设备摘要的 `aria-label` 同步更新（`device.summary`）。

## 7. 无障碍与一致性

- 最小字号 ≥ 13px，正文对比度 ≥ 4.5:1，状态色不单靠颜色传达（配图标）。
- 全界面键盘可达：向导、动作编辑器、确认弹窗均可纯键盘操作。
- macOS / Windows 视觉与交互一致；对话框居中、可 Esc 关闭。

## 8. 验证与埋点

### 8.1 用户测试（每阶段完成后的验收动作）

任务脚本（面向 5 名非技术用户）：

1. 让键盘上的一个键打出"你好"，然后按出来。
2. 把另一个键设成打开微信/浏览器。
3. 重命名键盘。

记录每步卡点、口头困惑、放弃点。目标：任务 1 在 3 分钟内无指导完成。

### 8.2 应用内埋点（阶段 3 起）

- 向导各步骤停留/放弃率（`DeviceSetupWizard` 各 stage 事件）。
- 从设备连接到首次成功按键的耗时。
- 首次会话中"自动识别按键"使用率。
- 按阶段对比埋点数据作为验收证据，参照 `docs/verification/` 现有验证文档风格记录。

### 8.3 回归

- 阶段 1 后跑 `make test`（前端测试与构建）；现有测试覆盖
  `DeviceSetupWizard`、`DeviceManagement`、`App` 等，术语改动会触碰快照断言，
  需同步更新测试文案断言。
- 阶段 2 向导重构涉及 `deviceSetupSession` 与向导状态机，需补充新流程的组件测试。

## 9. 验收标准

阶段 1：

- [ ] 侧栏与顶栏不再出现"设备管理 / 配置文件 / 就绪 / 需处理"等开发者文案。
- [ ] 设备详情的诊断字段默认折叠；普通用户默认视图看不到 GPIO/序列号/固件。
- [ ] 动作编辑器默认只显示"按下时"，触发方式在折叠区。
- [ ] `make test` 全绿（含更新后的文案断言）。

阶段 2：

- [ ] 新键盘向导只有 3 步，无"硬件配置"下拉框；自动完成分配。
- [ ] 预置模板可从空状态与新建流程直达。
- [ ] "自动识别按键"是接线默认入口。
- [ ] 一键刷机在受支持板卡上端到端可用；失败路径有说明书指引。

阶段 3：

- [ ] Windows 安装包已签名，无 SmartScreen 拦截。
- [ ] 非技术用户 5/5 完成 8.1 任务 1，无需指导。

## 10. 风险与边界

- 术语替换会触碰测试快照与 i18n 断言：阶段 1 内一并更新，避免半中半英。
- 向导重构不能破坏"多设备独立分配"语义（CONTEXT.md 的 Runtime Assignment 规则
  是领域不变量，UI 简化 ≠ 语义简化）。
- 一键刷机失败率必须可观测（进度 + 错误折叠区），否则比引导刷机更伤信任。
- Windows 签名需要证书与 CI 集成，独立排期，不阻塞阶段 1/2 的前端工作。
