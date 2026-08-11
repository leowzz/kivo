# Action 与设备工作区问题台账

> 更新日期：2026-08-10
> 状态：仍有未关闭问题，不应视为整体完成
> 关联规格：[`2026-08-10-action-device-workspace-followups.md`](../specs/2026-08-10-action-device-workspace-followups.md)
> 关联计划：[`2026-08-10-action-device-workspace-followups.md`](../plans/2026-08-10-action-device-workspace-followups.md)
> 原始设计：[`2026-08-09-action-triggers-and-device-workspace-design.md`](../specs/2026-08-09-action-triggers-and-device-workspace-design.md)

## 1. 文档用途

本文档只负责记录问题、风险、当前状态和关闭条件，不代替产品规格或实施计划。

- 实施计划中的任务完成，表示相应代码或测试已经执行，不等于用户问题已经关闭。
- 自动化测试通过，只能证明测试覆盖到的链路，不能代替 macOS 和实体设备验收。
- 问题只有在满足本文件的关闭条件并留下证据后，才可以改为“已关闭”。

状态定义：

- **待修复**：当前代码中仍有已确认缺陷。
- **待补证据**：已有部分实现，但测试没有覆盖用户实际经过的完整生产路径。
- **待实机验证**：自动化已覆盖部分链路，但实体设备和 macOS 行为尚未确认。
- **部分实机验证**：已有真实设备成功证据，但尚未覆盖完整输入、命令或发布固件矩阵。
- **已实现，待验收**：界面或交互已实现并通过自动化/浏览器检查，仍等待最终产品验收。
- **已确认设计**：产品关系已经定稿，不再作为开放方案讨论。

## 2. 用户反馈总览

| 反馈范围 | 问题 ID | 用户看到的问题 | 当前状态 |
| --- | --- | --- | --- |
| 设备管理 | UI-001 | 设备列表占用过大，没有形成“左侧窄表格、右侧大工作区” | 已实现，待验收 |
| 设备管理 | UI-002 / UI-003 | I/O 映射和按键布局不应放进弹窗；弹窗只保留触发设置和配置复制 | 已实现，待验收 |
| 通用界面 | UI-004 | 按钮退回浏览器原始样式，没有沿用应用现有按钮体系 | 已实现，待验收 |
| 行为编辑 | ACTION-006 | Action 卡片过大、留白多、信息密度低 | 已实现，待验收 |
| 快捷键 | HOTKEY-001 | `Command`、`Control`、`Option / Alt` 等技术键名被翻译或显示不一致 | 已关闭自动化缺口 |
| 快捷键 | HOTKEY-002 | “录制快捷键”藏在列表底部，不容易找到 | 已实现，待验收 |
| 快捷键 | HOTKEY-003 | 按键分类没有采用 tab 切换单一详情列表，界面拥挤且难选 | 已实现，待验收 |
| 快捷键 | HOTKEY-004 | 需要多选，并区分左/右 Command、Control、Option / Alt、Shift | 已实现，待验收 |
| 保存与生效 | SAVE-002 / SAVE-003 | 更改后没有自动保存，实体按键继续执行旧行为 | 已关闭 |
| Action 运行 | ACT-001 | macOS 粘贴 Action 不执行，也没有写入剪贴板 | 已关闭 |
| Action 运行 | ACT-002 | 等待 Action 没有产生配置的时间间隔 | 已关闭 |
| Action 运行 | ACT-003 | 媒体控制 Action 没有产生系统行为 | 已关闭 |
| 稳定性 | CRASH-001 | macOS `0.5.1` 在启动完成阶段崩溃 | 已关闭 |

## 3. P0 Action 问题结论

| ID | 优先级 | 状态 | 问题 |
| --- | --- | --- | --- |
| ACT-001 | P0 | 已关闭 | macOS 粘贴 Action 曾不写剪贴板，也不执行粘贴 |
| ACT-002 | P0 | 已关闭 | 等待 Action 曾不生效，后续步骤没有按预期间隔执行 |
| ACT-003 | P0 | 已关闭 | 媒体控制 Action 曾不生效 |

### ACT-001：粘贴 Action 在 macOS 上无效

**用户现象**

- 粘贴文本 Action 没有执行。
- 文本也没有写入系统剪贴板。
- 重新刷写固件后仍然无效，因此不能只把问题归因于固件版本。

**当前证据**

**状态：已关闭（2026-08-10）**

- Rust 协议测试和 runtime smoke 已覆盖部分 Paste 命令格式、ACK 顺序和失败处理。
- `2026-08-10 00:55:01 +0800` 的生产日志记录了 GPIO 9 / `K9`、run ID `71`：`Paste("OK.")` 步骤 `1/2` 完成，随后 `Enter` 步骤 `2/2` 完成；目标应用实际收到了对应文本。
- macOS `pbcopy` 另行完成了包含中文、换行和符号的 UTF-8 往返验证（24 个字符、38 字节）。
- 为序列号 `50031519384E811C` 创建了独立配置 `3key physical test 2026-08-10`，`KEY1` 绑定包含中文、换行和符号的 42 字符 Paste。生产日志记录 run `1` 至 `7` 以及 `35` 至 `37` 的 `1/1` Paste 开始和完成，用户确认目标端按键行为正确。
- 失败分支继续由 Rust 和 runtime smoke 回归覆盖；本次实机没有触发错误，因此没有把成功样本误写为失败路径验证。
- 证据日志：`/Users/leo/Library/Application Support/cn.wleo.kivo/log/kivo.log`。

**关闭条件**

1. 在 macOS 上配置包含中文、换行和符号的粘贴文本。
2. 实体按键触发后，系统剪贴板内容与配置文本完全一致。
3. 当前焦点应用收到粘贴内容。
4. 权限不足、剪贴板写入失败和设备通信失败能显示不同的明确错误。
5. 活动记录能关联设备、按键、Action、run ID 和最终结果。

### ACT-002：等待 Action 没有产生间隔

**用户现象**

- Action 序列包含等待时，没有观察到等待。
- 重新刷写固件后仍然没有等待或后续执行。

**当前证据**

**状态：已关闭（2026-08-10）**

- 协议、native firmware 和 runtime smoke 已覆盖 `DELAY` 命令及 `500ms` 参数。
- 独立实机配置把 `KEY2` 的按下行为设为 Paste -> Delay `500ms` -> Play/Pause。run `34` 在同一 run ID 内依次完成 `1/3`、`2/3`、`3/3`，用户确认可观察间隔和后续行为正确。
- 运行事件的 `timestampMs` 保留触发输入的捕获时间，不能用于反推每个 ACK 的墙钟间隔；因此时间验收采用用户的直接观察，命令顺序和参数由实体日志与协议回归交叉证明。

**关闭条件**

1. 实机配置 `粘贴 -> 等待 500ms -> 媒体播放/暂停`。
2. 同一个 run 内严格按 `1/3 -> 2/3 -> 3/3` 推进。
3. 实测第二步和第三步之间满足配置的最小等待时间。
4. 错误的 run ID 或 step ACK 不得推进序列。
5. 活动记录区分开始等待、等待完成和等待失败。

### ACT-003：媒体控制 Action 在 macOS 上无效

**用户现象**

- 播放/暂停等媒体控制没有产生系统行为。

**当前证据**

**状态：已关闭（2026-08-10）**

- 自动化已覆盖媒体 usage 编码、按下/释放传输和协议顺序。
- 实体 `KEY2` 已完成 Play/Pause（run `34`）、Next（run `11`、`13`、`15`）、Volume Up（run `19`、`21`）和 Volume Down（run `24`、`27`、`30`、`33`），用户确认对应系统行为正确。
- 测试期间长按/双击绑定从最初的 Previous/Next 改为 Volume Up/Volume Down，因此 Previous 没有独立实体样本；它与已验证命令共用 Consumer Control 报告与运行路径，native 测试继续覆盖 usage 的按下/释放传输。该覆盖说明不影响关闭“媒体控制整体无效”的用户问题。

**关闭条件**

1. 分别实测播放/暂停、上一首、下一首以及当前支持的其他媒体命令。
2. 每个命令都产生可观察的系统行为，或返回具体可操作的失败原因。
3. 媒体 Action 与 Paste、Delay 使用同一运行队列，不得被跳过或另起 run。

### CRASH-001：macOS 0.5.1 启动阶段崩溃

**已知报告**

- 应用版本：Kivo `0.5.1`
- 系统：macOS `15.7.7`
- 结果：主线程在 `tao::platform_impl::platform::app_delegate::did_finish_launching` 中触发 Rust panic，最终 `SIGABRT`。
- 原始附件：`/Users/leo/.codex/attachments/595def53-e09d-4f9e-8849-0172ec380617/pasted-text.txt`
- 最新用户复现：`/Users/leo/.codex/attachments/5868a577-3b74-4ce4-931f-982895fc29e2/pasted-text.txt`
- 带终端 backtrace 的复现报告：`/Users/leo/.codex/attachments/a8de7a9f-17bb-4083-976c-5437e8e97785/pasted-text.txt`

**当前状态**

**状态：已关闭（2026-08-10）**

终端直接启动 `/Applications/Kivo.app/Contents/MacOS/kivo` 后，完整 backtrace 给出了被 Finder 崩溃报告隐藏的原始原因：`Failed to setup app: error encountered during setup hook: unsupported_profile_schema`。当前数据中的 Device Profile 使用 schema 3，而旧安装包不支持该 schema；`Workspace::load` 的错误被返回给 Tauri，随后在 macOS `did_finish_launching` 回调中 panic，并因该回调不可 unwind 而 `SIGABRT`。

修复后的启动边界不再向 Tauri 返回初始化错误。运行日志在工作区加载前安装；失败会记录 `application_startup_failed`，后端发布稳定错误状态，前端显示“配置由较新版本创建、现有配置未修改”的阻断页面。失败页面不加载运行时快照、不订阅设备事件，也不会启动自动保存或覆盖配置。

使用独立 bundle ID `cn.wleo.kivo.startup-test` 构建并签名了 release macOS `.app`：有效 schema 到达 `application_ready`，窗口和托盘初始化成功；未来 profile schema 冷启动和重复启动均保持运行、显示 `unsupported_profile_schema` 页面，且 `~/Library/Logs/DiagnosticReports` 没有新增 Kivo `.ips` 报告。旧 `0.5.1` 二进制本身不会被原地改变，用户需要用包含本修复的新构建替换它。

**关闭条件**

1. 使用包含当前修改的正式 macOS 构建冷启动和重复启动。
2. 验证主窗口、菜单栏和托盘初始化不会触发 panic。
3. 若仍崩溃，保留带符号信息的 panic 原因，而不只记录 `abort() called`。

以上三项均已满足。

## 4. 本轮已关闭的审查问题

### SAVE-003：设备管理中的非当前编辑配置不会自动保存

**状态：已关闭（2026-08-10）**

**补充发现**

- 原自动保存目标只跟随全局“当前编辑配置”，在设备管理中修改另一个设备实际使用的配置时，该草稿可能一直不进入保存队列。
- 同时修改多个可自动保存的配置时，单目标保存模型无法正确表达部分成功、后续失败和重试；失败后可能重复写入已经保存成功的版本。
- 显式保存一个旧版本期间，如果产生了更新的共享草稿，旧请求完成后不得清除新草稿的共享保护状态。

**修复后的规则**

1. 自动保存按所有 dirty 配置文件计算，不再只检查全局当前编辑配置。
2. 共享待确认、采集中、学习中、无效的草稿继续被 gate 排除，不能静默保存。
3. 同一批可保存配置按批次执行；只有整个批次成功后才统一应用后端权威快照。
4. 批次中前面的配置已成功、后面的配置失败时，错误保持可见并可重试；重试跳过已经持久化的相同版本。
5. 只有当前草稿仍等于已持久化版本时才能清理 dirty/shared 状态，旧请求不得覆盖更新草稿。

**关闭证据**

- `App.test.tsx` 新增 `autosaves a uniquely assigned managed profile that is not the Editor Profile`。
- `App.test.tsx` 新增 `keeps a multi-profile autosave failure visible and retryable`。
- `App.test.tsx` 新增 `keeps a newer shared draft gated while an older explicit save completes`。
- 修改后的聚焦前端验证为 69 项通过，`npm run build` 通过，相关 diff check 通过。

### SAVE-002：行为修改后没有自动保存，设备继续使用旧行为

**状态：已关闭（2026-08-10）**

**用户现象**

- 在按键行为页面修改 Action 后，界面显示了新内容，但没有自动保存。
- 随后按下实体按键时，设备仍执行旧行为。

**本轮明确的保存规则**

1. 按键行为页面中的 Action 内容、触发类型、顺序等普通编辑进入统一防抖自动保存。
2. 页面跳转必须等待正在进行的自动保存完成，不能让未完成保存被导航打断。
3. 保存成功后显示“已自动保存”，并采用后端返回的权威快照更新界面状态。
4. 已分配设备收到同一份新配置：纯 Action 变化更新运行时快照；需要更新固件配置的变化进入重新配置流程。
5. 保存失败时保留草稿、显示失败状态并允许重试。
6. 只有在设备管理中真正修改了被多个设备共享的 I/O 映射或按键布局时，才暂停自动保存并要求显式保存或复制；仅进入设备管理页面不能影响后续行为编辑的自动保存。

**关闭证据**

- `App.test.tsx` 覆盖 Action 列表创建、排序、触发类型和快捷键编辑进入 `save_device_profile`，并验证页面跳转会等待保存完成。
- 回归测试证明访问共享配置的设备管理页面后，按键行为编辑仍会自动保存并显示“已自动保存”。
- `useAutosave.test.tsx` 覆盖串行保存、保存期间产生新版本、失败重试及成功状态保持。
- Rust 测试 `live_update_save_sends_action_only_snapshot_to_assigned_workers` 证明保存后的 Action 快照会发送给已分配设备；需要更高协议版本时则由 `live_update_reconfigures_when_an_action_requires_newer_firmware` 覆盖重新配置路径。
- 实体设备随后使用自动保存后的专用配置完成复杂 Paste、Delay 500ms 与多个 Media 命令，证明编辑结果已到达活动运行时；对应证据记录在 `ACT-001`、`ACT-002`、`ACT-003`。

### SAVE-001：共享配置草稿可绕过显式保存

**状态：已关闭（2026-08-10）**

**确认的危险路径**

1. 两个或更多设备共享同一个配置文件。
2. 在设备管理中修改该共享配置的 I/O 映射或按键布局。
3. 不点击“保存共享配置”，也不点击“复制并仅用于此设备”。
4. 直接离开设备管理页面。
5. 页面切换后手动保存 gate 消失，同一 dirty 草稿可能进入普通防抖自动保存，静默覆盖所有共享设备。

**修复前代码位置**

- `src/App.tsx`：`sharedProfileNeedsExplicitSave` 修复前依赖 `view === "devices"`。
- `src/useAutosave.ts`：`flush()` 对无效 target 直接完成，不会阻止导航。

**关闭条件**

1. “存在待确认的共享配置草稿”必须是独立状态，不能依赖当前页面。
2. 离开设备管理后，该草稿仍不得进入普通自动保存。
3. 只有“保存共享配置”“复制并仅用于此设备”或明确丢弃，才能解除 gate。
4. 没有待确认共享草稿时，按键行为页面仍保持正常自动保存。
5. 新增回归测试必须完整重现上述五步危险路径，并证明不会静默保存。

**关闭证据**

- `pendingSharedDraftProfileIds` 现在只在设备管理真正修改共享配置时产生，不再由打开设置或切换 tab 触发。
- pending gate 不再依赖当前页面；离开设备管理后仍保持，直到显式保存或复制。
- 回归测试完整覆盖“修改共享 I/O -> 离开 -> 等待 550ms -> 不保存 -> 返回 -> 显式保存”。
- 同时保留“只访问设备管理后，按键行为仍自动保存”的反向回归。

### TEST-001：缺少生产运行链路的组合测试

**状态：已关闭（2026-08-10）**

**修复前覆盖范围**

- `src-tauri/src/protocol.rs` 的序列测试直接构造 `ActionSequence` 并手工 ACK。
- `scripts/smoke_runtime_protocol.py` 由脚本指定 run ID 并发送命令。
- `src-tauri/src/device.rs` 当时已有生产 `DeviceSession` 的 Action 测试，但尚未用一个用例把 deferred clipboard、Delay 和 Media 串成用户报告的完整组合。

**风险**

现有测试能证明命令编码和固件 ACK 行为，但还不能单独证明：真实输入事件进入生产 `DeviceSession` 后，剪贴板延迟处理、等待和媒体会使用同一个由主机创建的 run，并且只接受正确的 `DONE` 推进。

**关闭条件**

新增一个聚焦的 `DeviceSession` 测试，至少证明：

1. 从真实 `DeviceMessage::State` 输入事件开始。
2. 解析当前配置中的 `Paste -> Delay 500ms -> Media Play/Pause`。
3. 只创建一个 host run ID。
4. 经过 deferred clipboard reply 路径后发出精确的 `PASTE <run> 1 3`。
5. 只在正确 `DONE` 到达后依次发出 `DELAY <run> 2 3 500` 和 `MEDIA <run> 3 3 205`。
6. 错误 run ID、重复 step 或跳步不能推进运行。

**关闭证据**

- 新增 `device::tests::protocol_v6_device_session_runs_deferred_paste_delay_and_media_in_one_host_run`。
- 测试从输入事件 ID `700` 开始，由生产 `DeviceSession` 创建 host run `1`。
- 错误 paste grant 不发送命令；正确 grant 发出 `1/3`，随后正确 `DONE` 依次发出 `DELAY 1 2 3 500` 和 `MEDIA 1 3 3 205`。
- 既有 `malformed_ack_aborts_only_active_v6_run_and_starts_queued_occurrence` 继续证明错误 Action ACK 不会推进当前 run。

### HOTKEY-001：技术键名显示不一致

**状态：已关闭（2026-08-10）**

**当前不一致**

| Token | 快捷键选择器 | Action 摘要 |
| --- | --- | --- |
| `alt` | `Option / Alt` | `Option` |
| `left_alt` | `Left Option / Alt` | `Left Option` |
| `primary` | `Primary (Command / Control)` | `Cmd/Ctrl` |

`src/HotkeyPicker.tsx` 维护了单独的 modifier label map，而 `src/ActionEditor.tsx` 通过 `src/hotkey.ts` 的 `formatHotkey()` 生成摘要，因此同一 token 出现两套名称。

**关闭条件**

1. 在 `src/hotkey.ts` 中保留唯一的技术键名格式化入口。
2. 选择器、已选 chip、录制结果、Action 摘要和运行日志复用同一规则。
3. 中文界面只翻译分类和说明，不翻译协议键 token。
4. 左右 `Command`、`Control`、`Option / Alt`、`Shift` 始终可区分。
5. 新增选择器与 Action 摘要一致性的回归测试。

**关闭证据**

- `src/hotkey.ts` 新增唯一的 `hotkeyDisplayLabel()`，`formatHotkey()` 与 `HotkeyPicker` 均复用该入口。
- picker、chip 和 Action 摘要现在统一显示 `Primary (Command / Control)`、`Option / Alt`、`Left Option / Alt`、`Right Option / Alt`。
- `hotkey.test.ts` 与 `ActionEditor.test.tsx` 增加一致性回归，浏览器也验证了 picker chip 和摘要。

## 5. 已实现但仍需保留回归的问题

| ID | 状态 | 已实现内容 | 必须保留的回归标准 |
| --- | --- | --- | --- |
| UI-001 | 已实现，待验收 | 设备管理改为左侧 `320-360px` 窄设备表、右侧弹性工作区 | 1120、1440 和 390 宽度无页面横向溢出 |
| UI-002 | 已实现，待验收 | I/O 映射和按键布局位于页面工作区，不放在弹窗 | 切换设备和配置文件后仍编辑正确对象 |
| UI-003 | 已实现，待验收 | 设备设置弹窗只承载触发设置和配置复制 | 不把 I/O 或布局重新塞回弹窗 |
| UI-004 | 已实现，待验收 | 业务按钮使用 primary、secondary、danger 基础样式 | 不出现浏览器原生灰色按钮；focus、disabled 可辨认 |
| HOTKEY-002 | 已实现，待验收 | 录制快捷键入口移到搜索和分类上方 | 无需滚动到底即可开始、取消和重新录制 |
| HOTKEY-003 | 已实现，待验收 | 常用、功能键、字母、数字、符号、导航、小键盘使用 tab | 同时只渲染一个 panel，tab/tabpanel IDREF 有效 |
| HOTKEY-004 | 已实现，待验收 | 支持多选和独立左右修饰键 | 左右 Command/Control/Alt/Shift 不被合并 |
| ACTION-004 | 已实现，待验收 | 单个 Action 可选择按下、释放、长按、双击；默认按下 | 不再出现独立“短按”类型 |
| ACTION-005 | 已实现，待验收 | 单个 Action 内容用弹窗编辑，按键和全部 Action 对应关系留在独立页面 | 弹窗不承担全局按键浏览职责 |
| ACTION-006 | 已实现，待验收 | Action 编辑卡片已压缩，提升信息密度 | 小屏不溢出，主要命令不被隐藏到卡片底部 |

## 6. 已确认的产品关系

以下内容是已经确认的设计边界，不再作为开放问题反复调整：

1. **配置文件**继续存在，作为复制、分享、导入、导出和同步的载体。
2. **配置文件页面**用于管理配置文件，但不再放“当前编辑配置”的全局切换控件。
3. **设备管理页面**选择当前正在管理的设备，并为这个设备选择实际使用的配置文件。
4. 多个设备可以共享一个配置文件；修改共享内容时必须明确显示影响范围。
5. **按键行为页面**选择“我要编辑的配置文件”，并浏览所有按键及其 Action。
6. **单个 Action 编辑**使用弹窗；I/O 映射和按键布局使用设备管理页面内的工作区。
7. **触发类型**保留按下、释放、长按、双击；默认是按下，不保留独立短按。
8. 拿起/放下听筒这类 GPIO 稳态输入分别映射为按下和释放，因此可以各自绑定不同 Action。

## 7. 当前验证基线

本轮已有的自动化与浏览器证据：

- 最新完整前端基线：17 个测试文件，207 项测试通过；前端构建通过。
- Rust：263 项通过、1 项忽略，另有 2 项集成测试通过。
- Runtime smoke：26 项测试通过；Rust 格式检查通过。
- Native firmware：57 项测试通过。
- 前端构建、Rust clippy、ESP32-S3 构建、RP2040 构建和 `git diff --check` 通过。
- 浏览器已重新检查 1120x760、1440x900 和 390x844：设备列表桌面端为 `360px`，右侧工作区分别为 `560px` 和 `880px`；移动端两区以 `390px` 上下堆叠，页面无横向溢出。快捷键弹窗保持单一 tab panel、顶部录制入口、稳定英文技术键名和统一按钮样式，控制台无 warning/error。
- RP2040 当前可被系统识别，串口由运行中的候选 Kivo 进程持有。专用配置的复杂 Paste、Delay 500ms、Play/Pause、Next、Volume Up 和 Volume Down 均已有实体运行证据，用户确认按键行为正确。

以上基线已经关闭 `SAVE-001`、`SAVE-002`、`SAVE-003`、`TEST-001`、`HOTKEY-001`、`CRASH-001`、`ACT-001`、`ACT-002` 和 `ACT-003`。

## 8. 建议处理顺序

1. 将包含当前修复的新 macOS 构建替换仍会因 schema 3 崩溃的已安装 `0.5.1`。
