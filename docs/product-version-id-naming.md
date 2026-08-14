# 产品版本 ID 命名规范

本文定义 Kivo 实体产品的能力变体和硬件修订命名。Product Version ID
用于标识一套可对外承诺的产品能力及其硬件实现版本，不替代软件发布版本、
固件版本、Board Profile ID、Device ID 或 Device Profile ID。

## 身份分层

| 字段 | 含义 | 何时变化 |
|---|---|---|
| Product Family ID | 产品系列及代际 | 产品进入新的、不能视为同代变体的代际时 |
| Product Variant ID | 同一系列内不可变的控制器与用户可见能力组合 | 控制器系列、按键数量或已登记能力发生变化时 |
| Hardware Revision | 同一能力变体的 PCB、引脚或器件修订 | 实现变化但对外能力不变时 |
| Product Version ID | Product Variant ID 与 Hardware Revision 的完整组合 | 任一组成部分变化时 |

这些字段均不包含单台设备序列号。单台设备仍由 Device ID 标识。Product Variant ID
只记录控制器系列的稳定缩写；具体使用哪块开发板、如何接线以及运行哪个固件，仍分别由
Board Profile、Hardware Profile 和固件版本描述。

## 规范格式

```text
<product-family>-<controller>-k<key-count>[-<capability>...]-r<hardware-revision>
```

示例：

```text
workbench-one-rp-k18-mic-disp-encp-r01
```

组成规则：

- 全部使用小写 ASCII、数字和连字符，不使用空格或下划线。
- Product Family ID 使用稳定的产品名，不写按键数量、能力、PCB 修订或生命周期阶段。
- Controller token 表示控制器芯片系列，必须来自本文登记表；更换控制器系列会产生新的
  Product Variant ID。
- `kNN` 表示独立实体按键的数量。显示组件自带的确认/返回键、编码器旋转和编码器按压
  均不计入该数量；这些信号属于组件硬件配置。
- 能力 token 必须来自本文登记表，并严格按照登记顺序排列。
- Hardware Revision 使用 `rNN`，至少两位，从 `r01` 开始；持久化时可保存整数 `1`。
- Product Version ID 发布后保持不可变；不能通过改变既有 token 的含义来复用旧 ID。

推荐校验表达式：

```regex
^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$
```

## 控制器 Token

| Token | 控制器系列 |
|---|---|
| `rp` | RP2040 |
| `s3` | ESP32-S3 |

Token 只区分芯片系列，不携带开发板厂商信息。比如 YD-RP2040 使用 `rp`，
YD-ESP32-S3 使用 `s3`；将来同一芯片系列增加其他 Board Profile 时仍复用同一 token。

## 能力 Token

能力 token 只表达会影响用户选择、固件适配或配置兼容性的公开能力，不记录麦克风
芯片型号、屏幕尺寸、显示技术、编码器料号等实现细节。

| 顺序 | Token | 含义 |
|---:|---|---|
| 10 | `mic` | 产品提供麦克风输入 |
| 20 | `spk` | 产品提供扬声器输出 |
| 30 | `disp` | 产品提供集成显示屏 |
| 40 | `enc` | 产品提供只能旋转的编码器 |
| 40 | `encp` | 产品提供可旋转、可按压的编码器 |

`enc` 与 `encp` 互斥。增加新 token 时，先在此表中登记其唯一含义和排序位置，
再用于 Product Variant ID。

## 当前命名决定

Kivo Workbench One 的计划版本包含 18 个独立实体按键、麦克风、集成显示屏，
以及可旋转、可按压的编码器。当前命名记录如下：

```yaml
status: planned
display_name: Kivo Workbench One
product_family_id: workbench-one
product_variant_id: workbench-one-rp-k18-mic-disp-encp
hardware_revision: 1
product_version_id: workbench-one-rp-k18-mic-disp-encp-r01
```

`status: planned` 表示这是已确定的命名和目标能力，不代表硬件实现、固件支持或
实体设备验收已经完成。实现和验收完成后，Product Version ID 保持不变，只更新
独立的实现状态和验证记录。

## 变更示例

| 变化 | 结果 |
|---|---|
| 同一能力下修正 PCB 或更换兼容器件 | `workbench-one-rp-k18-mic-disp-encp-r02` |
| 控制器从 RP2040 改为 ESP32-S3 | `workbench-one-s3-k18-mic-disp-encp-r01` |
| 编码器改为不能按压 | `workbench-one-rp-k18-mic-disp-enc-r01` |
| 独立按键增加到 20 个 | `workbench-one-rp-k20-mic-disp-encp-r01` |
| 增加扬声器 | `workbench-one-rp-k18-mic-spk-disp-encp-r01` |
| 只升级固件 | Product Version ID 不变 |
| 推出不兼容的下一代产品 | 新建 Product Family，例如 `workbench-two` |

EVT、DVT、PVT、MP、颜色、地区、生产批次和单机序列号应使用独立字段；除非它们
改变了本文登记的产品能力，否则不进入 Product Version ID。
