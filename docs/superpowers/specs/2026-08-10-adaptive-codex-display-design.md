# Kivo 自适应 Codex 状态屏设计

日期：2026-08-10

状态：设计已口头确认，书面规格待复核

## 背景

Kivo 当前为 RP2040 键盘支持一块 `SSD1306 128x32` 单色 OLED。固件使用
U8g2 的 `6x13` 字体显示两行调试状态和最近 GPIO 输入，Helper 不会向屏幕发送
业务语义。日常使用时，`READY 18 KEYS` 和 GPIO 编号只能证明设备工作正常，不能
帮助用户判断正在进行的工作是否需要关注。

用户最常见的场景是同时运行多个 Codex 任务。屏幕应成为一个低干扰的 Codex
注意力入口：平时显示全局运行数，有任务等待输入时立即接管，任务结束后短暂提示
响应已就绪。它不是日志终端，也不尝试在 128x32 上复刻 Codex UI。

屏幕硬件未来可能更换为不同尺寸、宽高比或色深，因此信息来源、排版和面板驱动
必须分别演进。第一版采用内置的 Provider 和 Renderer 注册机制，不建设可安装的
第三方插件平台。

## 目标

- 聚合本机全部 Codex 任务，而不是只显示当前 Kivo 仓库的任务。
- 在 128x32 OLED 上优先显示运行数、等待用户输入和响应就绪事件。
- Provider 只产生与屏幕尺寸无关的语义数据。
- Helper 根据面板能力生成紧凑绘制指令，固件执行绘制和面板刷新。
- 以稳定区域、内容哈希和 revision 事务实现增量传输。
- SSD1306 使用 8x8 tile 局部刷新，减少 I2C 阻塞和按键扫描延迟。
- 固件本地故障在 Helper 不可用时仍能显示，并始终高于远程内容。
- 为后续新增内置信息源和内置面板类型保留清晰边界。

## 非目标

- 不提供插件发现、安装、升级、权限、隔离或第三方分发平台。
- 不加载动态库，也不执行 Provider 提供的任意绘制代码。
- 不把 Codex 对话正文、推理、工具参数或日志内容发送到键盘。
- 不显示逐 token 输出、滚动字幕、动画、CPU、天气或通用系统通知。
- 不把按键已经派发给 Codex 表述为 Codex 任务已经成功。
- 不在第一版实现图标、进度条、触摸、彩色位图或完整 framebuffer 传输。
- 不为尚未接入的 128x64、方屏或彩屏增加用户可选配置；只保证抽象边界可容纳它们。
- 不要求旧协议固件显示 Codex 内容；旧固件继续使用现有本地调试画面。

## 设计原则

1. **语义优先**：信息源表达“有三个任务运行”，不表达“第一行写什么”。
2. **能力驱动**：Renderer 依据面板能力选择内容和布局，不按 Provider 分叉。
3. **固件无领域知识**：固件理解区域、字体和文字，不理解 Codex 或任务。
4. **明确状态，不做猜测**：只有显式等待事件才显示 `NEEDS INPUT`；静默、耗时或
   `notLoaded` 不能被推断为等待。
5. **增量但可恢复**：正常路径只发送变化区域，任何版本错位都回到完整场景。
6. **本地故障优先**：远程内容不能遮盖配置错误、Helper 断开或固件异常。

## 总体架构

```text
CodexTaskSource
  |  Codex task snapshots/events
  v
CodexDisplayProvider ----+
                         |
Future built-in provider +--> DisplayHub --> DisplayRenderer
                                              |
                                              v
                                        DisplayScene
                                              |
                                              v
                                      DisplayTransport
                                              |
                                              v
                                  Firmware DisplayRuntime
                                              |
                                              v
                                        PanelDriver
```

### `CodexTaskSource`

负责取得 Codex 任务元数据和运行事件，并把不稳定的外部格式收敛成内部
`CodexTaskSnapshot`。它不决定显示优先级，也不生成屏幕文案。

### `DisplayProvider`

把一个领域的数据转换成 `DisplayItem`。第一版只注册
`CodexDisplayProvider`。后续 Kivo 动作回执或外部脚本是新的 Provider，而不是在
Codex Provider 内增加分支。

### `DisplayHub`

保存 Provider 的最新项目，统一处理去重、优先级、TTL、过期清理和来源失效。Hub
输出一个与设备无关的有序语义快照。

### `DisplayRenderer`

是纯函数：输入语义快照和 `DisplayCapabilities`，输出 `DisplayScene`。Renderer
负责选择内容、排版、截断、字符集降级和区域划分，不执行 I/O。

### `DisplayTransport`

保存每台设备最后确认的 scene revision，比较区域哈希，编码串口事务并处理确认、
重同步和断线。它不解释 Codex 状态。

### 固件 `DisplayRuntime`

解析并暂存一个显示事务，提交时重放已验证的绘制指令到 framebuffer，记录 dirty
tiles，再由主循环按预算调用面板驱动。固件本地状态仲裁也在这一层完成。

实现时应新增聚焦的 `display` 模块，不继续把 Provider、排版和协议状态堆入当前已经
很大的 `device.rs` 或 `lib.rs`。

## 语义模型

```text
DisplayItem
  id: String
  source: String
  priority: ambient | normal | attention | critical
  state: idle | running | needs_input | success | warning | error
  title: String
  detail: Option<String>
  metrics: Map<String, Integer>
  progress: Option<0..100>
  expires_at: Option<Instant>
  updated_at: Instant
```

约束：

- `id` 在同一 Provider 内稳定，例如 `codex.summary`、`codex.task.<thread_id>`。
- `updated_at` 只在语义内容变化时更新，轮询到相同值不能制造新事件。
- 持续状态没有 `expires_at`；瞬态事件必须有 TTL。
- Provider 输出原始 Unicode 文本；字符集处理只发生在 Renderer。
- `progress` 第一版可存在于模型中，但 128x32 Renderer 不显示，协议也不实现进度条。

Codex Provider 产生两类项目：

```text
codex.summary
  priority: ambient
  state: running | idle
  metrics:
    running: 3
    needs_input: 1
```

```text
codex.task.<thread_id>
  priority: attention
  state: needs_input | success | warning | error
  title: KIVO
  detail: user input requested | response ready | system error
  expires_at: depends on state
```

`needs_input` 持续到对应请求得到响应或任务结束；`response ready` 和用户中断保留
8 秒；显式系统错误保留 15 秒。结束只表示 Codex 已经给出响应，屏幕文案使用
`RESPONSE READY`，不能使用 `SUCCESS` 或 `DONE` 暗示用户目标已经完成。

## Codex 数据来源

### 官方元数据通道

Codex App Server 是官方提供的客户端集成边界。Helper 通过无网络监听的 stdio
子进程执行 `initialize`，再调用 `thread/list` 获取任务 ID、名称、cwd、更新时间和
状态。查询固定传入 `useStateDbOnly: true`，禁止 App Server 扫描 JSONL 并修复状态库，
从而保持 Kivo 集成只读。协议结构由当前安装版本的
`codex app-server generate-json-schema` 生成并作为兼容性测试输入，不手写臆测字段。

Helper 启动时完整分页一次；此后每 2 秒按 `updated_at desc` 增量读取，遇到上次已见
时间戳即可停止分页。成功查询只更新 source health，不修改 `DisplayItem.updated_at`。

官方 `ThreadStatus` 包含 `notLoaded`、`idle`、`systemError` 和 `active`；`active` 的
`activeFlags` 可明确给出 `waitingOnApproval` 或 `waitingOnUserInput`。如果 Helper 能连接
到拥有任务运行态的同一 App Server，这些字段是最高可信来源。参考：
[Codex App Server](https://learn.chatgpt.com/docs/app-server)。

### Codex Desktop 兼容通道

当前 Codex Desktop 为每个任务启动 `codex app-server --listen stdio://`。独立启动的
App Server 虽然能列出这些任务，但实测运行中的 Desktop 任务在独立进程里仍返回
`notLoaded`；运行态不是跨进程共享状态。因此第一版不能只靠另一个 App Server
轮询来判断 Desktop 任务是否正在运行。

为支持当前主要使用场景，`CodexTaskSource` 增加一个严格只读的 rollout watcher：

- 从 `initialize` 返回的 `codexHome` 定位 `sessions/YYYY/MM/DD/*.jsonl`，不硬编码
  `~/.codex`。
- 首次启动以 `thread/list` 返回的所有未归档 path 为候选：读取文件开头的元数据，
  再从尾部反向扫描到最近一个 turn 生命周期边界，不扫描对话正文。App Server
  不可用时降级为最近 24 小时内修改的 session 文件。之后以文件通知监听创建和追加，
  并用 1 秒低频 stat 轮询弥补通知丢失。cursor 持久化后不重复读取已处理部分。
- 从 `session_meta` 取得 thread ID 和 cwd；同时保留 App Server `thread/list` 的
  `name`，供未来较大屏幕使用。128x32 只显示 cwd basename，避免泄露任务正文。
- `event_msg.task_started` 打开一个 turn，`event_msg.task_complete` 关闭对应 turn。
- `event_msg.turn_aborted(reason=interrupted)` 关闭对应 turn，并发布瞬态 `warning`，
  不能把用户主动停止显示成系统错误。
- `response_item.function_call(name=request_user_input)` 到同 `call_id` 的
  `function_call_output` 之间标记 `needs_input`。
- 只有明确的 App Server `activeFlags` 才能标记 `waitingOnApproval`。rollout watcher
  不把普通工具运行或长时间无输出猜成等待审批。
- 未识别事件、截断的最后一行和未来新增字段全部忽略；解析失败不能影响 Helper 的
  设备 Runtime。
- watcher 每秒以“sessions 目录可读、所有 cursor 可继续读取”更新 source health；没有
  新任务事件是正常状态，不能触发 stale。
- 初始化扫描只恢复未闭合 turn/request；扫描前已经完成或中断的历史任务不重放为
  新瞬态事件。

rollout 格式不是稳定的公开 API，因此解析器必须独立于 Provider，并以已去除正文的
fixture 做版本兼容测试。未来 Codex Desktop 暴露可附加的共享 App Server socket 后，
替换 `CodexTaskSource` 即可，`DisplayItem`、Hub、Renderer 和固件协议均不变化。

### 状态归一化

| 外部事实 | 内部状态 | 屏幕含义 |
| --- | --- | --- |
| 至少一个未结束 turn | `running` | Codex 正在处理 |
| `waitingOnUserInput` 或未响应的 `request_user_input` | `needs_input` | 需要用户回答 |
| `waitingOnApproval` | `needs_input` | 需要用户批准 |
| `task_complete` | 瞬态 `success` | 响应已就绪，不代表业务成功 |
| `turn_aborted(reason=interrupted)` | 瞬态 `warning` | 任务被用户中断 |
| `systemError` | 瞬态 `error` | Codex 运行错误 |
| `idle` / `notLoaded` 且无未结束 turn | `idle` | 当前没有确认在运行 |

同一个 thread 同时满足多项时，`needs_input` 高于 `error` 瞬态，`error` 高于
`response ready`，这些状态都高于普通 `running`。

### 数据最小化

Helper 不读取或发送对话正文、推理、命令、文件变更、工具参数和最终回复内容。
rollout parser 只保留事件类型、thread/turn/call ID、时间和 cwd；App Server 只取
thread 名称及状态所需字段。屏幕标题经过控制字符清理和长度限制。

## Hub 仲裁与生命周期

排序键依次为：

1. 固件本地 `critical` 状态，由固件直接仲裁，不进入 Helper Hub。
2. Provider `critical`。
3. `needs_input`。
4. `error` 瞬态。
5. `response ready` 瞬态。
6. `interrupted` 瞬态。
7. 普通 `running` 汇总。
8. `idle` 汇总。

同级任务按 `updated_at` 降序，最后用稳定 `id` 排序，避免每次轮询重排。Hub 每次
输出完整语义快照；区域增量由 Renderer 和 Transport 完成。

Codex source 是组合健康状态：App Server 失败时 rollout watcher 仍可提供运行数和 cwd
标题；watcher 失败时 App Server 仍可提供它自己拥有的 active status。只有两个通道都
不可用才开始 stale 计时。超过 5 秒没有任何成功健康检查时标记 stale：清除该来源的
瞬态项目，不再延长 TTL，但保留最后汇总最多 15 秒并将其降级为 `warning`。超过
15 秒后移除 Codex 内容，Renderer 显示 `CODEX OFFLINE`。恢复后只发布当前快照，不
重放失效期间的完成事件。

每个启用 OLED 的在线设备默认订阅同一份 Codex 全局快照。第一版不增加按设备选择
Provider 的设置 UI。

## 面板能力与 Renderer

```text
DisplayCapabilities
  panel_id
  width_px
  height_px
  pixel_format
  rotation
  fonts[]
    id
    glyph_set
    metrics
  primitives[]
  max_scene_ops
  max_text_bytes
  partial_update
    mode: none | tile
    tile_width_px
    tile_height_px
```

第一版只有内置 `ssd1306_128x32_mono`：

- 128x32、1 bit、rotation 0。
- 字体 `u8g2_6x13_ascii`。
- 原语只有 `ClearRegion` 和 `Text`。
- 局部刷新为 8x8 tile。
- 当前 `Ssd1306Config { sda, scl }` 在运行时映射到这个 panel profile，不修改现有
  Device Profile YAML 格式。

非 0 度旋转时第一版回退全屏刷新，因为 U8g2 `updateDisplayArea` 忽略 rotation。
不支持局部更新的未来驱动声明 `mode: none`，同一个 Scene 仍可使用，只改变刷新策略。

### 128x32 布局

使用两个 16px 高的稳定行区。6px 字宽理论上可容纳 21 个 ASCII 字符；第一版单条
状态文案限制为 16 个字符，汇总首行则拆成两个独立区域。汇总画面细分为三个区域：

```text
row0_left   x=0   y=0  w=64  h=16   "CODEX"
row0_right  x=64  y=0  w=64  h=16   "3 RUN"
row1        x=0   y=16 w=128 h=16   "1 NEEDS INPUT"
```

文字 baseline 分别为 12 和 29。所有区域边界已经对齐 8x8 tile。运行数从 3 变成 4
时只改变 `row0_right`；等待数变化只改变 `row1`。

确定文案如下：

```text
CODEX       3 RUN
1 NEEDS INPUT
```

```text
KIVO
NEEDS INPUT
```

```text
KIVO
APPROVAL NEEDED
```

```text
KIVO
RESPONSE READY
```

```text
KIVO
TASK STOPPED
```

```text
CODEX IDLE
KIVO READY
```

```text
CODEX OFFLINE
KIVO READY
```

当任意 `needs_input` 存在时，先显示最近更新的具体任务；如果有多个，则右上角在可用
空间显示 `+N`。具体任务状态解除后回到汇总画面。项目标题优先使用清理后的 cwd
basename，并转为大写 ASCII；名称无法表示时使用 thread ID 前 4 位，例如
`TASK A3F2`。第一版不引入 CJK 字库，也不按 UTF-8 字节错误截断。

## Scene 与区域差分

```text
DisplayScene
  revision: u32
  regions[]
    slot: u8
    id: String
    bounds_px
    content_hash: u64
    draw_operations[]
```

- `id` 和 `slot` 由某个 Renderer 布局稳定定义；`id` 只存在于 Helper，协议发送紧凑
  `slot`。
- `content_hash` 覆盖区域边界、字体、文字和全部绘制参数。
- Transport 按设备保存最后一次 `DISPLAY_OK` 的 scene；未确认的 scene 不能成为下一
  次 delta 的 base。
- 相同 scene 不产生串口命令。
- delta 只包含哈希或边界变化的区域。
- Renderer 模式变化、面板变化、固件重启、Helper 重连或 resync 一律发送 full scene。
- revision 使用每台设备独立的单调 `u32`；溢出前主动执行 full scene 并从 1 重新开始。

## 串口协议

显示场景需要新的固件协议版本 7。Host 只有在 `HELLO` 表明协议至少为 7、硬件配置
包含 OLED 且面板能力匹配时才发送显示事务。协议小于 7 时不发送未知命令，也不把
设备判为错误。

事务格式：

```text
DISPLAY_BEGIN <new_rev> <base_rev> <full|delta>
DISPLAY_REGION <slot> <x> <y> <w> <h>
DISPLAY_CLEAR <slot>
DISPLAY_TEXT <slot> <x> <baseline_y> <font_id> <base64_text>
DISPLAY_COMMIT <new_rev>
```

响应格式：

```text
DISPLAY_OK <new_rev>
DISPLAY_RESYNC <current_rev>
DISPLAY_ERROR <new_rev> <error_code>
```

协议约束：

- 每行继续小于现有 255-byte 上限。
- 坐标是像素坐标；固件验证区域在面板范围内，Renderer 保证区域按刷新 tile 对齐。
- `DISPLAY_TEXT` 坐标必须落在对应 slot 区域内，字体必须是面板已声明的内置 ID。
- 文本先进行 UTF-8 和字符集验证，再以 base64 编码，解码后最多 48 bytes。
- 一个事务最多 8 个 region、24 个 draw ops；第一版实际最多 3 个 region、6 个 ops。
- 同一串口连接同时只允许一个事务。新的 `DISPLAY_BEGIN` 会丢弃未提交的旧事务。
- delta 的 `base_rev` 必须等于固件当前 logical revision；full 的 `base_rev` 固定为 0。
- 任一命令非法时丢弃整个暂存事务并返回 `DISPLAY_ERROR`，当前画面不变。
- `DISPLAY_COMMIT` revision 不匹配时同样丢弃事务。

固件不在收到每条指令时修改 framebuffer。它使用定长数组暂存并验证 draw ops；收到
合法 `COMMIT` 后先更新每个 slot 的已提交绘制指令，再一次性重放到 framebuffer、
合并 dirty tiles、更新 logical revision，最后返回 `DISPLAY_OK`。full scene 会清空
framebuffer 和全部旧 slot；delta 只替换事务中出现的 slot。保留的 slot 指令也用于
本地状态覆盖解除后的远程画面恢复。因此串口中断或半个事务不会产生半帧。

`DISPLAY_OK` 表示新 scene 已接受并进入刷新队列，不表示所有 I2C bytes 已发送完毕。
这一区别允许 Helper 继续合并后续状态，而不阻塞串口读循环。

## 固件刷新调度

SSD1306 128x32 的完整 framebuffer 为 512 bytes。100kHz I2C 下仅像素 payload 理论
耗时约 46ms，当前同步 `sendBuffer()` 会占用整个发送时间。第一版改为：

1. `COMMIT` 把变化区域转换成 tile dirty bitmap。
2. 主循环在完成一次按键扫描后，只发送预算内的连续 dirty tile。
3. 默认每轮最多发送一行 8px 高、连续 8 个 tile，即 64 bytes；具体时间预算通过
   真机测量确认，但不能让按键扫描间隔超过现有去抖要求。
4. 新 scene 到达时直接更新 framebuffer，并把尚未发送的 dirty bitmap 与新变化取
   并集；不会先发送已经过时的像素。
5. `needs_input`、错误和本地故障不做时间限频；普通计数最多 5Hz，未来进度最多 1Hz。
6. 同一 revision 的物理 tile 可能分多轮送出，但未提交事务永不进入 framebuffer。

U8g2 的 `updateDisplayArea(tx, ty, tw, th)` 使用 8x8 tile 坐标，只在 full-buffer 和
支持 U8x8 的显示器有效。固件必须自行验证边界，并在不满足能力时调用完整刷新。

## 本地状态仲裁

固件维护两个画面来源：`local_status` 和 `remote_scene`。

- 配置无效、OLED 初始化失败、Runtime 错误和明确的 Helper 断开属于本地 critical，
  立即覆盖 remote scene。
- 正常启动但尚未收到第一个 remote full scene 时继续显示当前本地启动/Ready 信息。
- Helper 连接断开后丢弃 remote revision 和未提交事务，显示 `HELPER OFFLINE`。
- Helper 重连后必须先完整同步；在 `DISPLAY_OK` 前不显示旧 remote scene。
- 本地 critical 覆盖期间继续接受并保存合法 remote scene，但不把它加入物理刷新
  队列。本地错误解除且连接仍有效时，重放最新已提交的 remote slot 指令并刷新。

固件只根据连接和本地 Runtime 事实判断 `HELPER OFFLINE`，不能根据 Codex Provider
是否健康推断 Helper 断开。

## 错误与恢复

- **Codex CLI 不存在或版本不兼容**：rollout watcher 继续用 cwd fallback 提供降级
  状态。只有 sessions 目录也不可读时，Hub 才最终显示 `CODEX OFFLINE`；设备按键
  Runtime 不受影响。
- **App Server 元数据失败**：保留 rollout watcher 的 thread ID/cwd 降级信息；不
  猜测 App Server active flags。
- **rollout 文件轮转或截断**：按 inode/path 重建 cursor，重新计算当前未结束 turn，
  不发历史完成事件。
- **单个 JSONL 事件损坏**：忽略该行并记录限频日志，继续读取后续换行完整事件。
- **Provider 卡住**：Hub 按 stale 规则降级和过期。
- **serial transaction 中断**：固件丢弃 staged ops，保持当前 logical revision。
- **revision 错位**：固件返回 `DISPLAY_RESYNC`，Helper 丢弃该设备的 acked scene 并发
  full scene。
- **区域或字体越界**：返回 `DISPLAY_ERROR`，不修改 framebuffer；连续错误只记录
  限频日志，不重启设备 Runtime。
- **设备重启**：固件 revision 回到 0，Helper 下一轮 full sync。

## 内部扩展方式

Provider 和 Renderer 使用静态注册表：

```text
ProviderRegistry
  codex -> CodexDisplayProvider

RendererRegistry
  ssd1306_128x32_mono -> MonoText128x32Renderer
```

新增内置信息源要实现 `DisplayProvider` 并增加测试；新增面板要实现 PanelDriver、声明
`DisplayCapabilities` 并注册 Renderer。两者不能互相引用。第一版不提供目录扫描、
manifest、动态加载或外部 Provider IPC；等出现第二个真实外部集成需求后再评估。

## 测试策略

### Codex source

- 使用生成的 App Server schema fixture 验证四种 `ThreadStatus` 和两个 active flag。
- `thread/list` 分页、重复 thread、重命名和 cwd fallback。
- rollout 的 `task_started -> task_complete` 打开和关闭 running。
- rollout 的 `turn_aborted(reason=interrupted)` 关闭 running 并产生 warning。
- `request_user_input -> function_call_output` 期间为 needs input。
- 截断 JSON、未知事件、文件轮转和 Helper 重启恢复。
- 独立 App Server 返回 `notLoaded` 时不覆盖 rollout 已确认的 running。
- parser fixture 不包含对话正文，测试断言也不记录正文。

### Provider 与 Hub

- 汇总计数、稳定 ID、同内容不更新时间。
- needs input、error、response ready 和 running 的优先级。
- 多个 waiting task 的稳定排序和 `+N`。
- 8 秒与 15 秒 TTL、5 秒 stale、15 秒 offline。
- Provider 恢复后不重放过期事件。

### Renderer

- 128x32 每种确定画面的 golden scene。
- 区域边界全部对齐 8x8 tile，文字不超出区域。
- 计数变化只改变 `row0_right` hash；等待数只改变 `row1` hash。
- 非 ASCII 标题使用 cwd ASCII basename 或 `TASK XXXX` fallback。
- 相同语义输入生成完全相同的 scene 和 hashes。

### Host protocol

- full scene、delta scene、ack 后才推进 base revision。
- 未变化 scene 产生零显示命令。
- 255-byte 行限制、base64、文本长度、最大 region/op 数。
- timeout、`DISPLAY_ERROR`、`DISPLAY_RESYNC` 和断线重连 full sync。
- 协议 6 设备不接收任何 `DISPLAY_*` 命令并维持现有行为。

### Firmware

- 未提交、非法和 revision 错位事务不修改 framebuffer。
- commit 后只标记变化区域 tile。
- 单个计数变化不发送整帧。
- 连续 scene 合并尚未发送的 dirty tiles。
- 不支持局部更新或非 0 rotation 时回退 full refresh。
- 本地 critical 状态覆盖并在解除后正确恢复 remote scene。
- 显示刷新调度不丢 GPIO 事件，不破坏 debounce、HID 和串口处理。

### 真机验收

在照片对应的 18 键 RP2040 + SSD1306 设备上验证：

1. 同时启动多个 Codex Desktop 任务，汇总 running 数正确。
2. `request_user_input` 出现后 1 秒内显示具体项目和 `NEEDS INPUT`。
3. 用户回答后恢复汇总；Codex 回应完成后显示 8 秒 `RESPONSE READY`。
4. 单个计数变化的 I2C bytes 明显少于 512-byte full frame。
5. 连续状态变化没有可见乱码，串口中断不出现未提交内容。
6. 高速按键期间局部刷新不造成可感知漏键或额外重复触发。
7. 拔掉 Helper、制造配置错误、重连后，本地优先级和 full resync 正确。

自动测试和编译成功不能替代上述物理屏幕可读性、刷新字节数和按键延迟验收。

## 交付顺序

1. 建立语义模型、Codex source、Provider 和 Hub，以日志/测试快照验证聚合结果。
2. 增加协议 7 的显示事务和固件 staged-op parser，不启用局部刷新。
3. 增加 128x32 Renderer、full scene 同步和本地状态仲裁。
4. 增加 region hash、delta、dirty tile 和预算刷新。
5. 完成 Codex Desktop rollout 兼容、真机测量和故障恢复验收。

每一步都保持旧协议设备可用。局部刷新只有在 full scene 路径通过后启用，便于把
内容正确性和性能问题分开定位。

## 验收标准

- 屏幕默认展示全部 Codex 任务的准确汇总，而不是调试 GPIO 状态。
- 显式等待用户输入能接管画面；状态解除后自动恢复。
- 任务结束只显示 `RESPONSE READY`，不虚构业务成功。
- Provider、Hub、Renderer、Transport 和固件 Driver 边界独立可测试。
- 现有配置自动映射为内置 `ssd1306_128x32_mono`，不要求用户迁移 YAML。
- Helper 只发送变化区域；相同 scene 零传输，revision 错位可完整恢复。
- SSD1306 rotation 0 使用 8x8 tile 局部刷新，其他能力正确回退。
- 未提交或非法事务永不改变当前 framebuffer。
- 本地设备故障和 Helper 离线始终高于 Codex 内容。
- Codex 数据读取是只读和最小化的，解析失败不影响键盘 Runtime。
- 协议 6 及更旧的受支持设备保持原有行为。
