# Kivo 设备添加与配置创建设计

日期：2026-08-01

## 背景

当前 macOS 已将新插入的 RP2040 识别为 `Kivo Keyboard RP2040`，硬件序列号为 `50031519384E811C`，系统通信端口为 `/dev/cu.usbmodem1101`。Kivo 将它列为“待处理设备”，但界面只展示原始序列号、控制器、运行模式、端口和错误，没有解释设备为什么不能绑定，也没有给出下一步。

现有后端只有在设备返回并通过 HELLO 协议校验后才会登记正式 Device。这个安全边界是正确的：候选设备不能在身份或固件协议尚未验证时绑定 Runtime Assignment。问题在于前端把候选状态表现成技术诊断死路。

另一个独立问题是“配置文件”页面只有选择、导入、导出、备份、恢复和删除，没有新建 Device Profile 的入口。用户无法在设备离线或固件异常时先创建键盘配置。

macOS 的 `/dev/cu.*` 是系统提供的 call-out 串口路径，不是另一块设备。它只应作为高级诊断信息出现。

## 目标

- 新候选设备首次出现时，自动打开一次集中式“添加键盘”向导。
- 用用户可理解的状态解释候选设备为何不能绑定，并提供重新检测。
- 固件未响应时允许用户先独立创建配置，不让设备状态阻塞离线编辑。
- 设备通过 HELLO 校验并登记后，让向导继续选择或创建配置并完成绑定。
- “配置文件”页面始终提供独立“新建配置”入口。
- 新配置支持复制已有配置或创建空白配置。
- 多个物理键盘可以复用同一个 Device Profile。
- 保持现有身份验证、精确板型兼容和单设备分配隔离规则。

## 非目标

- 本次不构建、打包、安装或修复 RP2040/ESP32-S3 固件。
- 本次不内置 UF2、`picotool`、PlatformIO 或其他刷写工具。
- 本次不允许候选设备绕过 HELLO 校验直接登记或绑定。
- 本次不重做现有设备列表、硬件映射或固件协议。
- 本次不把系统端口路径提升为用户可操作的设备身份。

## 产品术语

- **键盘 / 设备**：一块有稳定硬件身份的物理控制器，对应后端 Device。
- **键盘配置 / 配置文件**：可被多个物理设备复用的 Device Profile，包含布局、行为和 Hardware Profiles。
- **硬件配置**：Device Profile 内针对一个 Board Profile 的接线拓扑，对应 Hardware Profile。
- **待处理设备**：已被 USB 枚举发现、但尚未通过完整身份或 HELLO 校验的 Candidate。

界面不再用“一个串口”“一个 cu”描述设备。主流程显示板型、友好状态和序列号末尾；完整端口路径只在“技术详情”中标为“系统通信端口”。

## 选定方案

采用集中式添加向导。向导由当前权威 `AppSnapshot` 驱动，不在前端伪造已登记、已绑定或已就绪状态。

候选设备仍由现有 Runtime Coordinator 负责扫描、启动 worker、验证 HELLO 和登记。新增结构化候选问题分类及显式重新检测命令。固件处理留在应用外部；当外部处理完成并且同一硬件身份通过校验后，向导从候选步骤自动继续到配置与绑定步骤。

配置创建是独立、完整的持久化操作。用户从固件错误页选择“先新建配置”后，即使关闭设备向导，新配置也保留。最终设备设置只原子更新设备名称和 Runtime Assignment，不把已经成功创建的配置当作临时草稿回滚。

## 用户流程

### 自动打开规则

1. 新的在线 Candidate 或在线且 Unassigned 的正式 Device 首次进入当前插入周期时，自动打开一次添加向导。
2. 用户关闭向导后，不再为同一插入周期重复弹出；设备列表保留醒目的“继续设置”入口。
3. 插入记录按稳定硬件身份跨 Candidate/Device 状态保留。只有同一身份既不再是 Candidate、也不再是在线 Device 时才清除；Candidate 通过 HELLO 转成正式 Device 不视为拔线。
4. 多个新设备同时出现时只打开一个向导，其余设备显示“等待设置”，用户可从列表明确选择。
5. 已分配设备、离线设备和身份冲突设备不会触发普通绑定步骤。

自动打开和关闭抑制是当前 UI 会话状态，不写入 Workspace。设备、候选、配置和绑定仍完全来自后端快照。

### 候选设备步骤

候选问题由后端提供结构化类别，前端不解析原始错误字符串：

- `validating`：正在确认设备。
- `firmware_not_responding`：USB 已识别，但固件没有在时限内返回 HELLO。
- `firmware_incompatible`：设备返回 HELLO，但协议、控制器、板型或能力不兼容。
- `bootloader`：设备处于引导模式，尚不能作为键盘使用。
- `port_unavailable`：系统通信端口无法打开或被占用。
- `invalid_identity`：缺少或包含无效硬件序列号。
- `duplicate_identity`：多个观察结果声明同一硬件身份。
- `unknown`：保留原始错误，但不猜测修复方式。

固件相关状态显示：

> Kivo 固件未响应。设备可能尚未刷入匹配固件，或固件协议版本不兼容。处理固件后保持 USB 连接，Kivo 会自动重新检测。

可用操作：

- “重新检测”：请求后端立即重新扫描该精确 Candidate，并回到“正在确认设备”。后台周期扫描仍保留。
- “先新建配置”：进入独立配置创建流程，不创建 Device 或 Runtime Assignment。
- “稍后处理”：关闭向导并保留列表入口。
- “查看技术详情”：显示原始序列号、Board Profile、Controller Family、模式、系统通信端口和原始错误。

Invalid/Duplicate Identity 不允许重新绑定或按端口选择目标，只显示消除歧义所需的提示。

设备管理页标题区同时提供带 `Plus` 图标的“添加键盘”命令，供用户随时重新打开集中式向导。存在多个待设置目标时，第一步先选择精确设备；没有可设置设备时，向导显示等待连接，并允许转到独立“新建配置”。

### 配置选择与创建

设备通过 HELLO 校验后，现有登记流程把它变为在线、身份有效、Unassigned 的正式 Device。向导用稳定 Device ID 匹配之前的 Candidate，并自动进入配置步骤。

用户每次明确选择：

- 使用已有配置：只列出至少一个 Hardware Profile 与设备 Board Profile 精确匹配的 Device Profiles。
- 新建配置：选择“复制已有配置”或“空白配置”。

复制配置时深拷贝布局、行为和全部 Hardware Profiles，生成新的 Device Profile ID 和名称。Hardware Profile ID 只在 Device Profile 内有作用，因此可以保留；Device Profile ID 必须唯一且创建命令不得覆盖已有配置。

空白配置包含合法的空布局、空行为，以及一个与所选 Board Profile 匹配、没有输入拓扑的 Hardware Profile。从设备向导进入时板型固定为该设备板型；从“配置文件”页面进入时先选择板型。

配置显示名称由用户输入。稳定 ID 由后端在 Workspace 锁内生成：从可用 ASCII 名称片段或板型基名生成，空结果使用 `profile`，冲突时追加递增后缀。ID 创建后不随显示名称变化。

### 完成绑定

最后一步显示并确认：

- 键盘名称
- 物理设备板型和序列号末尾
- Device Profile
- Hardware Profile

“完成设置”只接受正式、在线、身份有效的 Device。后端再次验证 Hardware Profile 的 Board Profile 精确匹配，并在一个 Workspace 事务中保存设备名称和 Runtime Assignment。失败时两者都不改变；前端保留表单并显示可操作错误。

成功后应用权威快照，设备进入 Configuring/Ready 状态，并导航到该配置的硬件映射页面。

### 独立新建配置

“配置文件”页面在配置选择区域提供带 `Plus` 图标的“新建配置”命令。它复用向导中的复制/空白表单，但不要求设备在线，也不创建或修改任何 Runtime Assignment。创建成功后将新配置设为当前 Editor Profile，并进入布局或硬件编辑入口。

## 组件与边界

### 后端

`CandidateIssue` 是可序列化枚举，由 Coordinator 根据候选模式、身份维度和 worker 错误代码生成。`CandidateStatus` 保留 `latestError` 供技术详情使用，并新增结构化 `issue` 字段。

新增命令：

- `retry_candidate(device_id)`：只接受当前唯一且可寻址的候选身份，停止该身份的验证 worker，立即重扫并返回权威快照。它不登记、不绑定、不修改 Workspace。
- `create_device_profile(request)`：创建复制或空白配置，拒绝覆盖，保存后返回完整快照。
- `complete_device_setup(device_id, name, assignment)`：验证正式 Device、名称和精确板型兼容，在一个 Workspace 事务中更新名称和分配，返回完整快照。

现有 `save_device_profile` 继续用于编辑已有配置；新建操作必须走 create 命令，避免把 ID 冲突静默解释为覆盖。

### 前端

新增独立 `DeviceSetupWizard`，负责选择目标、步骤导航和表单草稿。它消费 `devices`、`candidates`、`boardProfiles` 和 `deviceProfiles`，通过回调调用三个新命令。`App` 负责自动打开策略和应用权威快照。

配置创建表单提取为可复用 `CreateDeviceProfileForm`，同时供添加向导和配置文件页面使用。它不直接编辑 Registry，也不复用 autosave；只有后端成功后才应用新快照。

`DeviceManagement` 继续展示所有设备和候选，但主列表移除端口列。候选详情先显示友好问题、说明和操作，完整系统通信端口和其他技术字段收进可展开区域；正式 Device 的端口同样只在技术详情中显示。正式 Unassigned Device 显示“继续设置”，不要求用户手动拼接两个下拉框来理解绑定流程；现有高级分配控件可保留用于已登记设备的后续改绑和修复。

## 数据一致性

- Candidate 在 HELLO 校验前永远不能获得 Runtime Assignment。
- 创建配置与绑定设备是两个可独立完成的用户动作。已创建配置不是半完成状态。
- `create_device_profile` 在持久化前完成 ID、布局、硬件板型和名称校验。
- `complete_device_setup` 在同一 Workspace 锁和事务内验证并写入名称与分配。
- 所有命令返回完整 `AppSnapshot`；前端不乐观更改设备身份、配置列表或绑定。
- 周期刷新不能覆盖正在填写的本地表单，但目标消失或身份改变时必须停止提交并显示状态变化。

## 错误与恢复

- 固件无响应或不兼容：显示明确提示，允许重新检测和先建配置；不提供刷写动作。
- 端口不可用：显示“系统通信端口不可用或被其他程序占用”，保留重试。
- 设备拔出：向导保留已创建配置，但禁用设备完成操作；重新插入同一 Device ID 后可继续。
- 候选转为正式 Device：不关闭向导，自动进入配置步骤。
- 配置 ID 冲突：后端生成下一个可用后缀；显式非法输入仍返回字段错误。
- 配置创建失败：不添加本地临时配置，保留表单。
- 最终绑定失败：名称和分配均不变化，保留确认页。
- 多设备：所有命令携带精确 Device ID，不按板型、端口或当前列表选择进行扇出。

## 测试设计

### Rust

- Candidate 错误码到 `CandidateIssue` 的完整映射，包括 timeout、protocol mismatch、bootloader、port failure、invalid 和 duplicate identity。
- `retry_candidate` 只重启一个精确候选，不影响同板型的其他 worker，不接受缺失或冲突身份。
- 复制配置保留内容但生成新 Device Profile ID，且绝不覆盖已有配置。
- 空白配置通过现有 schema 校验，并含一个精确板型的空 Hardware Profile。
- `complete_device_setup` 对名称和分配同时成功或同时回滚。
- 不兼容 Hardware Profile、离线/未知/候选 Device 和身份冲突均拒绝完成设置。

### React

- 新 Candidate 首次自动打开一次，关闭后显示“继续设置”，同一插入周期不重复弹出。
- 多个 Candidate 排队且可明确选择。
- 固件问题显示用户文案；`/dev/cu.*` 只出现在技术详情。
- 固件问题状态仍能复制或创建空白配置。
- Candidate 变为同一 Device ID 的正式 Device 后自动进入配置步骤。
- 复用配置只列出精确板型兼容项。
- 配置文件页面可独立新建配置，设备为空或候选异常时也不禁用。
- 最终设置只调用一次精确 Device 命令，失败时不改变可见名称或分配。

### 集成与构建

- 命令边界测试覆盖 create、retry 和 complete 返回的权威快照。
- 现有设备管理、分配、学习、autosave 和备份测试保持通过。
- 完整 `npm test`、Rust tests、TypeScript/Vite build 和 `git diff --check` 通过。

## 实机验收

在当前 RP2040 `50031519384E811C` 上验证：

1. Kivo 将它显示为一块待处理 RP2040，而不是“串口”或“cu 设备”。
2. 若 HELLO 未响应，主界面显示明确固件提示，原始 `/dev/cu.usbmodem1101` 只在技术详情中出现。
3. 固件异常期间可以独立复制或创建空白配置，并能重新打开编辑。
4. “重新检测”只针对该硬件身份，不干扰其他设备。
5. 若外部固件处理后设备通过 HELLO，向导自动继续，能够选择兼容配置并完成单设备绑定。

第 5 项依赖外部提供可通过 HELLO v3 的固件；本次实现不承担刷写操作。

## 成功标准

用户插入新 RP2040 后，不需要理解 Candidate、runtime、串口或 `/dev/cu.*` 才能知道当前问题和下一步。设备固件异常不会再阻止创建键盘配置；设备通过验证后，用户可以在同一个添加向导里完成配置选择和绑定。
