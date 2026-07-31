# Kivo Helper UI 视觉与布局优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Kivo helper 的 UI 从朴素灰绿工程风升级为精致现代浅色系（设计 token 驱动），并统一优化全部视图的布局节奏。

**Architecture:** 零新依赖。新建 `src/styles/` 目录（tokens/base/app/views 四个 CSS 文件）逐步替代 `src/App.css`；TSX 改动仅限表现层标记（图标包裹、卡片分组、toast、面包屑），不改组件 props、状态逻辑与 Tauri 命令面。

**Tech Stack:** React 19 + TypeScript + 纯 CSS 变量 + Vitest + Tauri 2

**Spec:** `docs/superpowers/specs/2026-07-30-ui-visual-refresh-design.md`

**测试红线（现有测试不得破坏，计划已逐一核对）：**
- `App.test.tsx:95-97`：「首页」按钮不得移入 `<nav aria-label="配置">`；激活态类名 `is-active` 保留
- `App.test.tsx:126`：`.heat-cell` 的 `textContent` 结构（label + presses + 日期）不变
- `App.test.tsx:129`：`按下 X` 日志格式不变
- `App.test.tsx:303`：学习面板保持 `<details>` 默认折叠
- `i18n.test.ts`：zh-CN 与 en-US 字典 key 严格对齐——新增 key 必须双语同时添加
- `Keypad.test.tsx:32`：`.key-group` 类名保留

---

### Task 1: 设计 Token 体系（tokens.css + base.css）

**Files:**
- Create: `src/styles/tokens.css`
- Create: `src/styles/base.css`
- Modify: `src/main.tsx`

- [ ] **Step 1: 创建 `src/styles/tokens.css`**

```css
:root {
  /* 灰阶（冷调，Radix 式 12 档） */
  --gray-1: #fbfcfb;
  --gray-2: #f5f7f6;
  --gray-3: #eef1ef;
  --gray-4: #e6eae7;
  --gray-5: #dce1de;
  --gray-6: #d0d6d2;
  --gray-7: #bac2bd;
  --gray-8: #97a19c;
  --gray-9: #79837e;
  --gray-10: #5e6863;
  --gray-11: #3c4641;
  --gray-12: #1a211e;

  /* 品牌绿 12 档（--green-9 为主色） */
  --green-1: #f4faf7;
  --green-2: #e7f4ee;
  --green-3: #d7ebe1;
  --green-4: #bcdccf;
  --green-5: #95c9b1;
  --green-6: #6bae91;
  --green-7: #469475;
  --green-8: #237a5d;
  --green-9: #177457;
  --green-10: #106048;
  --green-11: #0e4b39;
  --green-12: #0b382b;

  /* 警告 / 错误（各 3 档 + 危险按钮主色） */
  --amber-bg: #fdf6e7;
  --amber-border: #ecd9a8;
  --amber-text: #7a5b12;
  --red-bg: #fdf1f0;
  --red-border: #f0cfcc;
  --red-text: #9c332b;
  --red-strong: #a83b34;

  /* 表面分层 */
  --bg-app: var(--gray-2);
  --bg-surface: #ffffff;
  --bg-raised: #ffffff;
  --border-subtle: var(--gray-4);
  --border-default: var(--gray-5);
  --border-strong: var(--gray-6);

  /* 阴影（双层柔和） */
  --shadow-1: 0 1px 2px rgba(26, 33, 30, .05), 0 1px 3px rgba(26, 33, 30, .04);
  --shadow-2: 0 1px 2px rgba(26, 33, 30, .06), 0 6px 16px rgba(26, 33, 30, .08);
  --shadow-3: 0 4px 8px rgba(26, 33, 30, .06), 0 16px 48px rgba(26, 33, 30, .16);

  /* 字阶 / 字重 */
  --text-11: 11px;
  --text-12: 12px;
  --text-13: 13px;
  --text-14: 14px;
  --text-17: 17px;
  --text-20: 20px;
  --text-24: 24px;
  --weight-regular: 450;
  --weight-medium: 550;
  --weight-semibold: 650;
  --weight-bold: 750;

  /* 圆角 */
  --radius-6: 6px;
  --radius-8: 8px;
  --radius-10: 10px;
  --radius-12: 12px;

  /* 间距（4px 基网格） */
  --space-4: 4px;
  --space-8: 8px;
  --space-12: 12px;
  --space-16: 16px;
  --space-20: 20px;
  --space-24: 24px;
  --space-32: 32px;
}
```

- [ ] **Step 2: 创建 `src/styles/base.css`**

```css
:root {
  color: var(--gray-12);
  background: var(--bg-app);
  font-family: Inter, "PingFang SC", "Microsoft YaHei", system-ui, sans-serif;
  font-synthesis: none;
  letter-spacing: 0;
}

* { box-sizing: border-box; }
html, body, #root { width: 100%; height: 100%; overflow: hidden; }
body { margin: 0; min-width: 320px; }
button, input, select, textarea { color: inherit; font: inherit; letter-spacing: 0; }
button { cursor: pointer; }
button:disabled { cursor: default; opacity: .45; }

select, input, textarea {
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-8);
  outline: none;
  background: var(--bg-surface);
  transition: border-color .15s ease, box-shadow .15s ease;
}
select {
  appearance: none;
  padding-right: 28px !important;
  background: var(--bg-surface) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Cpath fill='%2379837e' d='m4 6 4 4 4-4z'/%3E%3C/svg%3E") no-repeat right 9px center / 12px;
}

/* 统一焦点环：2px 绿外发光 + 圆角跟随 */
select:focus, input:focus, textarea:focus, button:focus-visible {
  border-color: var(--green-8);
  box-shadow: 0 0 0 2px var(--green-3);
  outline: none;
}

/* 细滚动条 */
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { border: 3px solid transparent; border-radius: 6px; background: var(--gray-6); background-clip: content-box; }
::-webkit-scrollbar-thumb:hover { background: var(--gray-7); background-clip: content-box; }
::-webkit-scrollbar-track { background: transparent; }
```

- [ ] **Step 3: 修改 `src/main.tsx` 导入新样式（App.css 暂留最后，过渡期内兜底）**

将 `import "./App.css";` 替换为：

```tsx
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/app.css";
import "./styles/views.css";
import "./App.css";
```

（`app.css` / `views.css` 在 Task 2、Task 4 创建；本步骤先创建两个空文件占位，避免导入报错：`touch src/styles/app.css src/styles/views.css`）

- [ ] **Step 4: 从 `src/App.css` 删除已迁移的规则**

删除 `:root` 块（第 1-7 行）、全局 reset（第 9-14 行）、`select, input, textarea` 与 `select`、`:focus` 规则（第 33-35 行）。

- [ ] **Step 5: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（本次为纯样式迁移，无行为变化）

- [ ] **Step 6: Commit**

```bash
git add src/styles/ src/main.tsx src/App.css
git commit -m "feat: add design token system and base styles"
```

---

### Task 2: 顶栏 + 侧栏（app.css）

**Files:**
- Modify: `src/styles/app.css`
- Modify: `src/App.tsx`（顶栏连接胶囊、侧栏结构）
- Modify: `src/App.css`（删除已迁移规则）

- [ ] **Step 1: 在 `src/styles/app.css` 写入壳层样式**

```css
/* ===== 壳层 ===== */
.product-shell { height: 100dvh; min-height: 0; display: grid; grid-template-rows: 48px minmax(0, 1fr); background: var(--bg-app); }

/* ===== 顶栏 48px ===== */
.topbar { display: grid; grid-template-columns: 200px 1fr auto; align-items: center; border-bottom: 1px solid var(--border-subtle); background: var(--bg-surface); }
.brand { height: 48px; display: flex; align-items: center; gap: 9px; padding: 0 var(--space-16); }
.brand img { width: 26px; height: 26px; border-radius: var(--radius-6); }
.brand h1 { margin: 0; font-size: var(--text-17); font-weight: var(--weight-bold); letter-spacing: -.01em; }

/* 连接状态：胶囊徽章 + 状态点 */
.connection { justify-self: start; display: flex; align-items: center; gap: 7px; min-width: 0; margin-left: var(--space-8); padding: 4px 12px; border: 1px solid var(--border-default); border-radius: 999px; color: var(--gray-10); background: var(--bg-app); font-size: var(--text-12); }
.connection .status-dot { width: 7px; height: 7px; flex: 0 0 auto; border-radius: 50%; background: var(--gray-7); }
.connection.is-connected { border-color: var(--green-4); color: var(--green-11); background: var(--green-1); }
.connection.is-connected .status-dot { background: var(--green-9); animation: pulse-dot 2s ease-in-out infinite; }
@keyframes pulse-dot { 0%, 100% { box-shadow: 0 0 0 0 rgba(23, 116, 87, .35); } 50% { box-shadow: 0 0 0 4px rgba(23, 116, 87, 0); } }
.connection code { max-width: 280px; overflow: hidden; text-overflow: ellipsis; color: var(--gray-9); font-size: var(--text-11); white-space: nowrap; }

.save-state { min-width: 130px; display: flex; align-items: center; justify-content: flex-end; padding-right: var(--space-16); color: var(--gray-9); font-size: var(--text-12); }
.save-state.is-saving::before { content: ""; width: 6px; height: 6px; margin-right: 6px; border-radius: 50%; background: var(--gray-8); animation: pulse-dot 1.2s ease-in-out infinite; }
.save-state.is-error { color: var(--red-text); }
.save-state button { margin-left: 8px; border: 0; padding: 2px 0; color: inherit; background: transparent; text-decoration: underline; }

/* ===== 侧栏 200px ===== */
.sidebar { min-height: 0; display: flex; flex-direction: column; padding: var(--space-12) 10px; border-right: 1px solid var(--border-subtle); background: var(--bg-app); }
.home-nav-button, .sidebar nav button, .data-nav-button, .data-menu button {
  position: relative; width: 100%; min-height: 34px; display: flex; align-items: center; gap: 9px;
  border: 0; border-radius: var(--radius-8); padding: 7px 10px;
  color: var(--gray-11); background: transparent; text-align: left; font-size: var(--text-13);
  transition: background .12s ease;
}
.home-nav-button:hover, .sidebar nav button:hover, .data-nav-button:hover, .data-menu button:hover { background: var(--gray-3); }
.home-nav-button.is-active, .sidebar nav button.is-active, .data-nav-button.is-active { color: var(--green-11); background: var(--green-3); font-weight: var(--weight-semibold); }
/* 激活态左侧 3px 指示条 */
.home-nav-button.is-active::before, .sidebar nav button.is-active::before, .data-nav-button.is-active::before {
  content: ""; position: absolute; left: -10px; top: 8px; bottom: 8px; width: 3px; border-radius: 2px; background: var(--green-9);
}
.sidebar nav { display: grid; gap: 2px; margin-top: var(--space-12); padding-top: var(--space-12); border-top: 1px solid var(--border-subtle); }
.sidebar nav > span { padding: 0 10px var(--space-4); color: var(--gray-9); font-size: var(--text-11); font-weight: var(--weight-semibold); letter-spacing: .04em; }
/* 「配置文件」固定底部 */
.data-nav-button { margin-top: auto; }

/* ===== 工作区 ===== */
.product-workspace { grid-row: 2; min-height: 0; height: 100%; display: grid; grid-template-columns: 200px minmax(360px, 1fr) minmax(320px, 380px); }
.product-workspace.is-home { grid-template-columns: 200px minmax(0, 1fr); }
.content-panel { min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-app); }

/* 统一页面头：64px，标题 + 副标题 + 右侧操作区 */
.content-heading { min-height: 64px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-20); padding: 10px var(--space-24); border-bottom: 1px solid var(--border-subtle); background: var(--bg-surface); }
.content-heading span, .panel-title span { display: block; margin-bottom: 2px; color: var(--gray-9); font-size: var(--text-11); }
.content-heading h2, .panel-title h2, .action-panel > h2 { margin: 0; font-size: var(--text-17); font-weight: var(--weight-semibold); line-height: 1.3; letter-spacing: -.01em; }

/* ===== 通用按钮 ===== */
.add-actions button, .source-actions button, .primary-button, .confirm-actions button, .layout-add-button, .learning-button, .empty-workspace button {
  min-height: 34px; display: inline-flex; align-items: center; justify-content: center; gap: 6px;
  border: 1px solid var(--border-strong); border-radius: var(--radius-8); padding: 6px 12px;
  background: var(--bg-surface); font-size: var(--text-12); font-weight: var(--weight-medium);
  box-shadow: var(--shadow-1); transition: box-shadow .15s ease, border-color .15s ease, background .12s ease;
}
.add-actions button:hover, .source-actions button:hover, .confirm-actions button:hover, .layout-add-button:hover, .learning-button:hover, .empty-workspace button:hover { border-color: var(--gray-7); box-shadow: var(--shadow-2); }
.primary-button { border-color: var(--green-9) !important; color: #fff; background: var(--green-9) !important; }
.primary-button:hover { background: var(--green-10) !important; }
.danger-button { border-color: var(--red-strong) !important; color: #fff; background: var(--red-strong) !important; }

.icon-button { width: 30px; height: 30px; display: inline-grid; place-items: center; border: 1px solid transparent; border-radius: var(--radius-6); padding: 0; color: var(--gray-9); background: transparent; transition: background .12s ease, color .12s ease; }
.icon-button:hover:not(:disabled) { color: var(--gray-11); background: var(--gray-3); }
.icon-button.is-danger { color: var(--red-text); }
.icon-button.is-danger:hover:not(:disabled) { background: var(--red-bg); }

/* ===== 空工作区 ===== */
.empty-workspace { min-height: 0; flex: 1; display: grid; place-items: center; align-content: center; gap: 10px; padding: 30px; color: var(--gray-9); text-align: center; }
.empty-workspace h2 { margin: 0; color: var(--gray-11); font-size: var(--text-17); }
.empty-workspace > div { display: flex; gap: 8px; }

/* ===== 响应式 ===== */
@media (max-width: 980px) {
  .product-workspace { grid-template-columns: 180px minmax(360px, 1fr); }
  .product-workspace.is-home { grid-template-columns: 180px minmax(0, 1fr); }
  .topbar { grid-template-columns: 180px 1fr auto; }
  .action-panel { grid-column: 1 / -1; min-height: 380px; border-top: 1px solid var(--border-subtle); border-left: 0; }
  .action-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(270px, 1fr)); gap: 10px; }
  .action-item { margin: 0; }
  .home-dashboard { min-height: 480px; flex: initial; grid-template-columns: 1fr; overflow: visible; }
  .activity-log { min-height: 260px; border-top: 1px solid var(--border-subtle); border-left: 0; }
}

@media (max-width: 680px) {
  .product-shell { grid-template-rows: auto minmax(0, 1fr); }
  .topbar { min-height: 48px; grid-template-columns: 120px 1fr; }
  .connection { justify-self: end; margin-left: 0; margin-right: var(--space-12); }
  .connection code { display: none; }
  .save-state { grid-column: 1 / -1; min-height: 0; padding: 0 var(--space-12) 6px; }
  .product-workspace { display: block; }
  .sidebar { display: grid; grid-template-columns: 1fr; border-right: 0; border-bottom: 1px solid var(--border-subtle); }
  .home-nav-button { width: auto; }
  .sidebar nav { display: flex; overflow-x: auto; margin-top: 10px; padding-top: 10px; border-top: 0; }
  .sidebar nav > span { display: none; }
  .sidebar nav button, .data-nav-button { flex: 0 0 auto; width: auto; white-space: nowrap; }
  .home-nav-button.is-active::before, .sidebar nav button.is-active::before, .data-nav-button.is-active::before { left: 10px; right: 10px; top: auto; bottom: 0; width: auto; height: 3px; }
  .content-panel { min-height: 480px; }
}
```

- [ ] **Step 2: 修改 `src/App.tsx` 顶栏连接胶囊（第 301-305 行）**

```tsx
<div className={connected ? "connection is-connected" : "connection"}>
  <span className="status-dot" aria-hidden="true" />
  {connected ? <Cable size={14} /> : <Unplug size={14} />}
  <span>{t(language, connected ? "connection.connected" : "connection.searching")}</span>
  {connection.port && <code>{connection.port}</code>}
</div>
```

- [ ] **Step 3: 修改 `src/App.tsx` 壳层网格行（第 298 行）**

`product-shell` 的 grid 行由 CSS 控制；删除第 314-322 行的 `error-banner` 块（Task 3 会以 toast 形式重新加入——本任务内先删，Task 3 紧邻执行，中间状态无错误展示，可接受）。同时第 324 行 `product-workspace` 的类名逻辑不变。

同时侧栏（第 325-347 行）：删除 `.data-nav-button` 上由 CSS 旧规则提供的 `margin-top: 14px; border-top; padding-top`（已在新 CSS 中改为 `margin-top: auto`），TSX 结构不变。

- [ ] **Step 4: 从 `src/App.css` 删除已迁移规则**

删除：`.product-shell`、`.topbar`、`.brand*`、`.connection*`、`.save-state*`、`.error-banner`、`.product-workspace*`、`.sidebar*`、`.home-nav-button`、`.data-nav-button`、`select/input/textarea` 残留、`.content-panel`、`.content-heading*`、`.icon-button`、`.empty-workspace*`、通用按钮组规则、两个 `@media` 块。保留：`.data-page*`、`.model-picker`、`.home-*`、`.metric-*`、`.heatmap*`、`.heat-cell*`、`.activity-log*`、`.keypad*`、`.key-*`、`.action-*`、`.panel-*`、`.field-*`、`.hotkey-*`、`.record-button`、`.add-actions`、`.hardware-*`、`.debounce-*`、`.source-*`、`.mapping-*`、`.contact-*`、`.learning-*`、`.safety-*`、`.pin-*`、`.modal-*`、`.confirm-*`、`.layout-*`、`.icon-row`。

- [ ] **Step 5: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（`is-active`、`nav` 结构、按钮角色均未变）

- [ ] **Step 6: Commit**

```bash
git add src/styles/app.css src/App.tsx src/App.css
git commit -m "feat: restyle topbar, sidebar and app shell with tokens"
```

---

### Task 3: 错误横幅 → 浮动 toast

**Files:**
- Modify: `src/App.tsx`（toast 标记）
- Modify: `src/styles/app.css`（toast 样式）

- [ ] **Step 1: 在 `src/App.tsx` 的 `</header>` 之后、`product-workspace` 之前插入 toast（替代 Task 2 删除的 error-banner）**

```tsx
{(error || runtimeError) && (
  <div className="error-toast" role="alert">
    <span>{error ?? runtimeError?.detail ?? runtimeError?.code}</span>
    <button className="icon-button" type="button" aria-label={t(language, "common.close")} onClick={() => {
      setError(null);
      setRuntimeError(null);
    }}><X size={15} /></button>
  </div>
)}
```

- [ ] **Step 2: 在 `src/styles/app.css` 追加 toast 样式**

```css
/* ===== 错误 toast（浮动，不挤压工作区） ===== */
.error-toast {
  position: fixed; z-index: 30; top: 58px; right: var(--space-16);
  max-width: min(460px, calc(100vw - 32px));
  display: flex; align-items: center; justify-content: space-between; gap: var(--space-12);
  padding: 10px 8px 10px var(--space-16);
  border: 1px solid var(--red-border); border-radius: var(--radius-10);
  color: var(--red-text); background: var(--red-bg);
  box-shadow: var(--shadow-2); font-size: var(--text-13);
}
.error-toast .icon-button { color: inherit; }
.error-toast .icon-button:hover:not(:disabled) { background: rgba(156, 51, 43, .1); }
```

- [ ] **Step 3: 运行测试**

Run: `npm test`
Expected: 全部 PASS（无测试断言 error-banner；`role="alert"` 与关闭按钮 aria-label 保留）

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx src/styles/app.css
git commit -m "feat: replace error banner with floating toast"
```

---

### Task 4: 首页仪表盘（views.css）

**Files:**
- Modify: `src/styles/views.css`
- Modify: `src/HomeDashboard.tsx`（指标卡图标包裹、热力图阶梯色）
- Modify: `src/App.css`（删除已迁移规则）

- [ ] **Step 1: 在 `src/styles/views.css` 写入首页样式**

```css
/* ===== 首页仪表盘 ===== */
.home-dashboard { min-height: 0; flex: 1; display: grid; grid-template-columns: minmax(0, 1fr) minmax(300px, 340px); overflow: hidden; }
.home-main { min-width: 0; min-height: 0; overflow: auto; padding-bottom: var(--space-24); }
.home-heading { margin-bottom: var(--space-16); }
.home-device { display: flex; align-items: center; gap: 7px; padding: 4px 12px; border: 1px solid var(--border-default); border-radius: 999px; color: var(--gray-10); background: var(--bg-app); font-size: var(--text-12); }
.home-device.is-connected { border-color: var(--green-4); color: var(--green-11); background: var(--green-1); }
.home-device code { max-width: 190px; overflow: hidden; text-overflow: ellipsis; color: var(--gray-9); white-space: nowrap; font-size: var(--text-11); }

/* 指标卡（主区内容限宽 880px，与页面头左对齐） */
.metric-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--space-16); max-width: 880px; padding: 0 var(--space-24); }
.metric-card {
  min-height: 108px; display: grid; align-content: start; gap: var(--space-8);
  border: 1px solid var(--border-subtle); border-radius: var(--radius-10); padding: var(--space-16);
  color: var(--gray-10); background: var(--bg-surface); font-size: var(--text-12);
  box-shadow: var(--shadow-1); transition: box-shadow .15s ease, transform .15s ease;
}
.metric-card:hover { box-shadow: var(--shadow-2); transform: translateY(-1px); }
.metric-icon { width: 28px; height: 28px; display: grid; place-items: center; border-radius: var(--radius-8); color: var(--green-9); background: var(--green-3); }
.metric-card strong { color: var(--gray-12); font-size: 28px; font-weight: var(--weight-bold); font-variant-numeric: tabular-nums; letter-spacing: -.02em; }

/* 热力图（5 档阶梯色） */
.heatmap-section { max-width: 880px; margin: var(--space-16) var(--space-24) 0; border: 1px solid var(--border-subtle); border-radius: var(--radius-10); background: var(--bg-surface); box-shadow: var(--shadow-1); }
.heatmap { display: grid; gap: 10px; padding: var(--space-12); }
.heatmap-group { display: grid; gap: 7px; }
.heat-cell {
  min-height: 68px; display: grid; align-content: space-between;
  border: 1px dashed var(--border-strong); border-radius: var(--radius-8); padding: 7px 9px;
  color: var(--gray-11); background: var(--gray-2); font-size: var(--text-11);
}
.heat-cell strong { font-size: var(--text-17); font-variant-numeric: tabular-nums; }
.heat-cell small { color: inherit; font-size: 10px; opacity: .75; }
.heat-cell.heat-1 { border: 1px solid var(--green-3); background: var(--green-2); color: var(--green-12); }
.heat-cell.heat-2 { border: 1px solid var(--green-4); background: var(--green-3); color: var(--green-12); }
.heat-cell.heat-3 { border: 1px solid var(--green-5); background: var(--green-4); color: var(--green-12); }
.heat-cell.heat-4 { border: 1px solid var(--green-6); background: var(--green-5); color: var(--green-12); }

/* 活动日志（卡片化条目） */
.activity-log { min-width: 0; min-height: 0; display: flex; flex-direction: column; border-left: 1px solid var(--border-subtle); background: var(--bg-surface); }
.panel-title { min-height: 64px; display: flex; align-items: center; justify-content: space-between; padding: 10px 18px; border-bottom: 1px solid var(--border-subtle); }
.panel-title strong { min-width: 24px; height: 24px; display: grid; place-items: center; border-radius: 12px; color: var(--green-11); background: var(--green-3); font-size: var(--text-11); font-variant-numeric: tabular-nums; }
.activity-log-list { min-height: 0; flex: 1; overflow: auto; padding: var(--space-8); }
.activity-log-item {
  display: flex; align-items: center; gap: 8px; min-width: 0;
  padding: 7px 8px; border-radius: var(--radius-6);
  color: var(--gray-11); font-size: var(--text-12); line-height: 1.35;
  transition: background .12s ease;
}
.activity-log-item:hover { background: var(--gray-2); }
.activity-log-item time { flex: 0 0 auto; color: var(--gray-8); font-size: var(--text-11); font-variant-numeric: tabular-nums; }
.activity-log-item span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.panel-empty { display: grid; place-items: center; align-content: center; gap: 6px; min-height: 120px; color: var(--gray-8); font-size: var(--text-13); text-align: center; }
.panel-empty svg { color: var(--gray-7); }
.home-unavailable { margin: var(--space-20) var(--space-24); padding: 10px 14px; border: 1px solid var(--red-border); border-radius: var(--radius-8); color: var(--red-text); background: var(--red-bg); font-size: var(--text-13); }

@media (max-width: 680px) {
  .metric-grid { grid-template-columns: 1fr; padding: 0 14px; }
  .heatmap-section { margin: 14px; }
  .home-heading { align-items: flex-start; flex-direction: column; gap: 8px; }
}
```

- [ ] **Step 2: 修改 `src/HomeDashboard.tsx`——指标卡图标包裹（第 37-39 行）**

```tsx
<div className="metric-card"><span className="metric-icon"><MousePointer2 size={16} /></span><span>{t(language, "home.todayPresses")}</span><strong>{metrics.todayPresses}</strong></div>
<div className="metric-card"><span className="metric-icon"><Hash size={16} /></span><span>{t(language, "home.activeButtons")}</span><strong>{metrics.activeButtonCount}</strong></div>
<div className="metric-card"><span className="metric-icon"><Trophy size={16} /></span><span>{t(language, "home.topButton")}</span><strong>{buttonLabel(model, metrics.topButton?.buttonId)}</strong></div>
```

- [ ] **Step 3: 修改 `src/HomeDashboard.tsx`——热力图改阶梯色 class（第 45-51 行）**

将 `.map` 回调中的单元格改为（内部 `span/strong/small` 结构与文本保持原样，`textContent` 测试不受影响）：

```tsx
{model?.model.groups.map((group) => <div className="heatmap-group" key={group.id} style={{ gridTemplateColumns: `repeat(${group.columns}, minmax(0, 1fr))` }}>
  {group.buttons.map((button) => {
    const entry = heatmapByButton.get(button.id);
    const presses = entry?.presses ?? 0;
    const level = presses === 0 ? 0 : Math.min(4, Math.max(1, Math.ceil((presses / maxHeat) * 4)));
    return <div className={level ? `heat-cell heat-${level}` : "heat-cell"} key={button.id} title={`${button.label}: ${presses}`}>
      <span>{button.label}</span>{presses > 0 && <><strong>{presses}</strong><small>{entry?.day.slice(5)}</small></>}
    </div>;
  })}
</div>)}
```

`maxHeat` 保留（阶梯色仍按最大值归一化）。

- [ ] **Step 3b: 修改 `src/HomeDashboard.tsx`——活动日志空状态加图标（第 60 行）**

`Activity` 图标已在文件顶部导入。将空状态改为：

```tsx
<p className="panel-empty"><Activity size={18} />{t(language, "activity.empty")}</p>
```

- [ ] **Step 4: 从 `src/App.css` 删除已迁移规则**

删除：`.home-*`、`.metric-*`、`.heatmap*`、`.heat-cell*`、`.activity-log*`、`.panel-title`、`.panel-empty`、`.content-heading` 残留（若有）。保留：`.data-page*`、`.model-picker`、`.keypad*`、`.key-*`、`.action-*`、`.field-*`、`.hotkey-*`、`.record-button`、`.add-actions`、`.hardware-*`、`.debounce-*`、`.source-*`、`.mapping-*`、`.contact-*`、`.learning-*`、`.safety-*`、`.pin-*`、`.modal-*`、`.confirm-*`、`.layout-*`、`.icon-row`。

- [ ] **Step 5: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（`.heat-cell` textContent 与 `按下 X` 日志断言不变）

- [ ] **Step 6: Commit**

```bash
git add src/styles/views.css src/HomeDashboard.tsx src/App.css
git commit -m "feat: refresh home dashboard with metric cards and stepped heatmap"
```

---

### Task 5: 按键行为页（键盘区 + 动作面板 + 选中面包屑）

**Files:**
- Modify: `src/i18n.ts`（新增 `behavior.selected`）
- Modify: `src/App.tsx`（面包屑胶囊）
- Modify: `src/ActionEditor.tsx`（动作卡片类型图标）
- Modify: `src/styles/views.css`
- Modify: `src/App.css`（删除已迁移规则）

- [ ] **Step 1: 在 `src/i18n.ts` 新增双语 key**

zhCN 中 `"behavior.title": "按键行为",` 之后插入：

```ts
"behavior.selected": "已选中：{label}",
```

enUS 中 `"behavior.title": "Button behavior",` 之后插入：

```ts
"behavior.selected": "Selected: {label}",
```

- [ ] **Step 2: 修改 `src/App.tsx` 行为页头部（第 410-412 行区域）**

在 `content-heading` 内、`view === "layout"` 按钮旁加入面包屑：

```tsx
<div className="content-heading">
  <div><span>{activeConfig.model.name}</span><h2>{t(language, view === "layout" ? "layout.title" : "behavior.title")}</h2></div>
  {view === "behavior" && selectedButton && <span className="selected-crumb">{t(language, "behavior.selected", { label: selectedButton.label })}</span>}
  {view === "layout" && <button className="primary-button" type="button" onClick={() => setLayoutEditorOpen(true)}><LayoutGrid size={16} />{t(language, "layout.edit")}</button>}
</div>
```

- [ ] **Step 3: 修改 `src/ActionEditor.tsx` 动作卡片头部加类型图标（第 96-97 行）**

```tsx
<div className="action-item-header">
  <span>{action.type === "paste" ? <TextCursorInput size={13} /> : <Keyboard size={13} />}{index + 1}. {t(language, action.type === "paste" ? "behavior.paste" : "behavior.hotkey")}</span>
```

- [ ] **Step 4: 在 `src/styles/views.css` 追加键盘区与动作面板样式**

```css
/* ===== 选中面包屑 ===== */
.selected-crumb { display: inline-flex; align-items: center; padding: 4px 12px; border: 1px solid var(--green-4); border-radius: 999px; color: var(--green-11); background: var(--green-2); font-size: var(--text-12); font-weight: var(--weight-medium); }

/* ===== 键盘区 ===== */
.keypad-stage { min-height: 0; flex: 1; display: grid; place-items: safe center; overflow: auto; padding: var(--space-32); }
.keypad { width: min(100%, 560px); height: min(100%, 685px); display: flex; flex-direction: column; gap: 18px; }
.key-group { flex-basis: 0; display: grid; gap: 9px; }
.key-button {
  position: relative; min-width: 0; min-height: 44px; display: grid; place-items: center;
  border: 1px solid var(--border-strong); border-radius: var(--radius-8); padding: 8px;
  color: var(--gray-12); background: var(--bg-surface);
  box-shadow: 0 1px 0 var(--gray-5), 0 2px 4px rgba(26, 33, 30, .06);
  font-size: var(--text-14); font-weight: var(--weight-semibold); overflow: hidden;
  transition: box-shadow .12s ease, transform .12s ease, border-color .12s ease, background .12s ease;
}
.key-button:hover { border-color: var(--green-6); background: var(--green-1); box-shadow: 0 1px 0 var(--gray-5), 0 4px 10px rgba(26, 33, 30, .08); transform: translateY(-1px); }
.key-button.is-selected { border-color: var(--green-8); background: var(--green-2); box-shadow: 0 0 0 2px var(--green-3), 0 1px 0 var(--green-7); }
.key-button.is-pressed { color: #fff; background: var(--green-9); transform: translateY(1px); box-shadow: 0 1px 0 var(--green-11); transition: transform .12s cubic-bezier(.34, 1.56, .64, 1), background .12s ease; }
.key-button small { position: absolute; top: 5px; right: 5px; min-width: 18px; height: 18px; display: grid; place-items: center; border-radius: 9px; color: #fff; background: var(--green-9); font-size: 10px; font-variant-numeric: tabular-nums; }
.key-button.is-pressed small { color: var(--green-9); background: #fff; }

/* ===== 动作面板 ===== */
.action-panel { min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; border-left: 1px solid var(--border-subtle); background: var(--bg-surface); }
.action-panel > h2 { padding: 20px 18px; border-bottom: 1px solid var(--border-subtle); }
.action-list { min-height: 0; flex: 1; overflow: auto; padding: var(--space-12); }
.action-item { margin-bottom: 10px; border: 1px solid var(--border-subtle); border-radius: var(--radius-10); background: var(--bg-surface); box-shadow: var(--shadow-1); }
.action-item-header { min-height: 38px; display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 5px 4px 12px; border-bottom: 1px solid var(--border-subtle); color: var(--gray-11); font-size: var(--text-12); font-weight: var(--weight-semibold); }
.action-item-header > span { display: inline-flex; align-items: center; gap: 6px; }
.action-item-header > span svg { color: var(--gray-8); }
.icon-row { display: flex; align-items: center; gap: 2px; }
.field-stack { display: grid; gap: 6px; padding: 11px; color: var(--gray-10); font-size: var(--text-11); }
.field-stack textarea { width: 100%; resize: vertical; min-height: 80px; padding: 8px; color: var(--gray-12); font-size: var(--text-13); line-height: 1.5; }
.field-error { color: var(--red-text); }
.hotkey-field { display: grid; grid-template-columns: 1fr auto; gap: 7px 10px; align-items: center; padding: 11px; }
.hotkey-field > span { color: var(--gray-10); font-size: var(--text-11); }
.hotkey-field output { grid-column: 1 / -1; min-height: 34px; display: flex; align-items: center; border: 1px solid var(--border-default); border-radius: var(--radius-6); padding: 0 9px; color: var(--gray-12); background: var(--gray-2); font-size: var(--text-12); font-variant-numeric: tabular-nums; }
.hotkey-manual { grid-column: 1 / -1; display: grid; gap: 7px; }
.hotkey-modifiers { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; }
.hotkey-modifiers label { min-height: 30px; display: flex; align-items: center; justify-content: center; gap: 4px; border: 1px solid var(--border-default); border-radius: var(--radius-6); color: var(--gray-11); font-size: var(--text-11); }
.hotkey-modifiers input { margin: 0; }
.hotkey-manual select { width: 100%; min-width: 0; height: 32px; padding: 0 7px; }
.record-button { grid-column: 1 / -1; min-height: 32px; display: flex; align-items: center; justify-content: center; gap: 7px; border: 1px solid var(--border-strong); border-radius: var(--radius-6); background: var(--bg-surface); font-size: var(--text-12); }
.record-button.is-recording { border-color: var(--amber-border); color: var(--amber-text); background: var(--amber-bg); }

/* 添加动作：虚线 drop-zone 风格 */
.add-actions { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; padding: var(--space-12); border-top: 1px solid var(--border-subtle); background: var(--bg-app); }
.add-actions button { border-style: dashed; border-color: var(--border-strong); box-shadow: none; color: var(--gray-10); background: transparent; }
.add-actions button:hover { border-color: var(--green-7); border-style: dashed; color: var(--green-11); background: var(--green-1); box-shadow: none; }

@media (max-width: 680px) {
  .keypad-stage { padding: 20px 14px; }
  .action-panel { min-height: 440px; }
}
```

- [ ] **Step 5: 从 `src/App.css` 删除已迁移规则**

删除：`.keypad*`、`.key-group`、`.key-button*`、`.action-*`、`.panel-title`（残留）、`.panel-empty`（残留）、`.icon-row`、`.field-*`、`.hotkey-*`、`.record-button*`、`.add-actions*`。保留：`.data-page*`、`.model-picker`、`.hardware-*`、`.debounce-*`、`.source-*`、`.mapping-*`、`.contact-*`、`.learning-*`、`.safety-*`、`.pin-*`、`.modal-*`、`.confirm-*`、`.layout-*`。

- [ ] **Step 6: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（动作编辑器角色/名称断言不受影响；i18n 字典双语对齐）

- [ ] **Step 7: Commit**

```bash
git add src/i18n.ts src/App.tsx src/ActionEditor.tsx src/styles/views.css src/App.css
git commit -m "feat: refresh behavior view keypad, action panel and selection crumb"
```

---

### Task 6: 硬件映射页

**Files:**
- Modify: `src/i18n.ts`（新增 `hardware.advancedHint`）
- Modify: `src/HardwareMapping.tsx`（信号源图标、学习面板 summary 结构）
- Modify: `src/styles/views.css`
- Modify: `src/App.css`（删除已迁移规则）

- [ ] **Step 1: 在 `src/i18n.ts` 新增双语 key**

zhCN 中 `"hardware.advanced": "适配新设备",` 之后插入：

```ts
"hardware.advancedHint": "学习按键与 GPIO 的对应关系",
```

enUS 中 `"hardware.advanced": "Adapt new device",` 之后插入：

```ts
"hardware.advancedHint": "Learn how keys map to GPIO pins",
```

- [ ] **Step 2: 修改 `src/HardwareMapping.tsx`——信号源头部加图标**

第 1 行导入改为：

```tsx
import { Cable, LayoutGrid, Plus, Radio, SquareStop, Trash2 } from "lucide-react";
```

第 67-70 行 `source-heading > div` 改为：

```tsx
<div>
  {source.type === "direct" ? <Cable size={14} /> : <LayoutGrid size={14} />}
  <strong>{sourceName(language, source)}</strong>
  <code>{source.id}</code>
</div>
```

- [ ] **Step 3: 修改 `src/HardwareMapping.tsx`——学习面板 summary 与学习中进行态（第 185-187 行）**

```tsx
<details className="learning-panel">
  <summary>
    <Radio size={15} />
    <span>{t(language, "hardware.advanced")}</span>
    <small>{t(language, "hardware.advancedHint")}</small>
  </summary>
  <div className={learning ? "learning-controls is-learning" : "learning-controls"}>
```

（`<details>` 标签与默认折叠行为不变，对应测试不受影响；仅将原第 187 行 `learning-controls` div 的 className 改为条件式。）

- [ ] **Step 4: 在 `src/styles/views.css` 追加硬件映射样式**

```css
/* ===== 硬件映射 ===== */
.hardware-view { min-height: 0; flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.debounce-field { display: grid; grid-template-columns: auto 64px auto; align-items: center; gap: 6px; color: var(--gray-10); font-size: var(--text-11); }
.debounce-field input { width: 64px; height: 32px; padding: 0 6px; font-variant-numeric: tabular-nums; }
.source-list { min-height: 0; flex: 1; overflow: auto; padding: var(--space-16) var(--space-24) 2px; }
.source-editor { margin-bottom: 14px; border: 1px solid var(--border-subtle); border-radius: var(--radius-10); background: var(--bg-surface); box-shadow: var(--shadow-1); }
.source-heading { min-height: 46px; display: flex; align-items: center; justify-content: space-between; padding: 6px 8px 6px 14px; border-bottom: 1px solid var(--border-subtle); }
.source-heading > div { display: flex; align-items: center; gap: 8px; }
.source-heading > div svg { color: var(--green-8); }
.source-heading strong { font-size: var(--text-13); font-weight: var(--weight-semibold); }
.source-heading code { color: var(--gray-8); font-size: 10px; }
.compact-field { padding: 10px 12px; border-bottom: 1px solid var(--border-subtle); }
.compact-field input, .compact-field select { height: 32px; padding: 0 8px; }
.mapping-table { font-size: var(--text-12); }
.mapping-head, .mapping-row { display: grid; grid-template-columns: minmax(110px, 1fr) 150px; align-items: center; min-height: 44px; border-bottom: 1px solid var(--gray-3); }
.mapping-head { color: var(--gray-9); background: var(--gray-2); font-size: 10px; font-weight: var(--weight-semibold); letter-spacing: .04em; }
.mapping-head span { padding: 0 12px; }
.mapping-row:last-child { border-bottom: 0; }
/* 选中行：浅绿底 + 左侧 3px 绿条（与侧栏激活态统一） */
.mapping-row.is-selected { background: var(--green-2); box-shadow: inset 3px 0 0 var(--green-9); }
.mapping-row > button { align-self: stretch; border: 0; padding: 0 12px; background: transparent; text-align: left; }
.mapping-row > button:hover { color: var(--green-11); }
.mapping-row > input, .contact-inputs select { width: 68px; height: 28px; padding: 0 6px; font-variant-numeric: tabular-nums; }
.contact-inputs { display: flex; gap: 6px; }
.source-actions { display: flex; gap: 8px; padding: 0 var(--space-24) var(--space-16); }

/* 学习面板：折叠态为虚线引导卡片 */
.learning-panel { margin: 0 var(--space-24) var(--space-24); }
.learning-panel summary {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 14px; border: 1px dashed var(--border-strong); border-radius: var(--radius-10);
  color: var(--gray-10); background: var(--bg-surface); cursor: pointer;
  font-size: var(--text-12); font-weight: var(--weight-semibold); list-style: none;
  transition: border-color .15s ease, color .15s ease, background .15s ease;
}
.learning-panel summary::-webkit-details-marker { display: none; }
.learning-panel summary svg { color: var(--gray-8); }
.learning-panel summary small { margin-left: auto; color: var(--gray-8); font-size: var(--text-11); font-weight: var(--weight-regular); }
.learning-panel summary:hover { border-color: var(--green-7); color: var(--green-11); background: var(--green-1); }
.learning-panel summary:hover svg { color: var(--green-8); }
.learning-panel[open] summary { margin-bottom: 10px; border-style: solid; border-color: var(--border-subtle); }
.learning-controls { display: grid; gap: 13px; border: 1px solid var(--border-subtle); border-radius: var(--radius-10); padding: 14px; background: var(--bg-surface); box-shadow: var(--shadow-1); }
.learning-controls.is-learning { border-color: var(--amber-border); background: var(--amber-bg); animation: pulse-dot 2s ease-in-out infinite; }
.learning-controls .compact-field { border: 0; padding: 0; }
.safety-check { display: flex; align-items: flex-start; gap: 8px; padding: 10px 12px; border: 1px solid var(--amber-border); border-radius: var(--radius-8); color: var(--amber-text); background: var(--amber-bg); font-size: var(--text-12); line-height: 1.4; }
.safety-check input { width: 16px; height: 16px; margin-top: 1px; }
.learning-controls fieldset { border: 0; margin: 0; padding: 0; }
.learning-controls legend { margin-bottom: 7px; color: var(--gray-10); font-size: var(--text-11); }
.pin-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(54px, 1fr)); gap: 5px; }
.pin-grid label { display: flex; align-items: center; gap: 5px; border: 1px solid var(--border-default); border-radius: var(--radius-6); padding: 5px; font-size: var(--text-11); font-variant-numeric: tabular-nums; }
.learning-button { justify-self: start !important; }

@media (max-width: 680px) {
  .mapping-head, .mapping-row { grid-template-columns: minmax(90px, 1fr) 130px; }
}
```

- [ ] **Step 5: 从 `src/App.css` 删除已迁移规则**

删除：`.hardware-*`、`.debounce-*`、`.source-*`、`.mapping-*`、`.contact-*`、`.learning-*`、`.safety-*`、`.pin-*`、`.compact-field`。保留：`.data-page*`、`.model-picker`、`.modal-*`、`.confirm-*`、`.layout-*`。

- [ ] **Step 6: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（`<details>` 折叠断言不受影响；i18n 双语对齐）

- [ ] **Step 7: Commit**

```bash
git add src/i18n.ts src/HardwareMapping.tsx src/styles/views.css src/App.css
git commit -m "feat: refresh hardware mapping view and learning panel styling"
```

---

### Task 7: 数据页卡片分组

**Files:**
- Modify: `src/i18n.ts`（新增 3 个 `data.*` key）
- Modify: `src/App.tsx`（数据页结构）
- Modify: `src/styles/views.css`
- Modify: `src/App.css`（删除已迁移规则）

- [ ] **Step 1: 在 `src/i18n.ts` 新增双语 key**

zhCN 中 `"nav.data": "配置文件",` 之后插入：

```ts
"data.groupModel": "设备型号",
"data.groupTransfer": "导入 / 导出",
"data.groupDanger": "危险操作",
```

enUS 中 `"nav.data": "Configuration files",` 之后插入：

```ts
"data.groupModel": "Device model",
"data.groupTransfer": "Import & export",
"data.groupDanger": "Danger zone",
```

- [ ] **Step 2: 修改 `src/App.tsx` 数据页结构（第 351-381 行）**

将 `data-page-body` 内的内容重组为三张卡片（select 与 5 个按钮的 JSX 原样保留，仅删除按钮从 `data-menu` 移入危险卡）：

```tsx
<div className="data-page">
  <div className="content-heading"><div><h2>{t(language, "nav.data")}</h2></div></div>
  <div className="data-page-body">
    <section className="data-card">
      <h3>{t(language, "data.groupModel")}</h3>
      <label className="model-picker"><span>{t(language, "model.select")}</span><select
        aria-label={t(language, "model.select")}
        value={activeModel ?? ""}
        disabled={!loaded || models.length === 0}
        onChange={(event) => void run(t(language, "error.save"), () => saveSettings(event.target.value, language))}
      >
        {models.length === 0 && <option value="">{t(language, "model.empty")}</option>}
        {models.map((model) => <option value={model.model.id} key={model.model.id}>{model.model.name}</option>)}
      </select></label>
    </section>
    <section className="data-card">
      <h3>{t(language, "data.groupTransfer")}</h3>
      <div className="data-menu">
        <button type="button" onClick={() => void chooseImport()}><FileInput size={16} />{t(language, "nav.import")}</button>
        <button type="button" disabled={!activeConfig} onClick={() => void run(t(language, "error.export"), async () => {
          await autosave.flush();
          const path = await saveFile({ defaultPath: `${activeConfig?.model.id ?? "model"}.yaml`, filters: [{ name: "Kivo", extensions: ["yaml"] }] });
          if (path && activeConfig) await invoke("export_model", { id: activeConfig.model.id, path });
        })}><Upload size={16} />{t(language, "nav.export")}</button>
        <button type="button" disabled={models.length === 0} onClick={() => void run(t(language, "error.export"), async () => {
          await autosave.flush();
          const path = await saveFile({ defaultPath: "kivo-backup.yaml", filters: [{ name: "Kivo", extensions: ["yaml"] }] });
          if (path) await invoke("export_backup", { path });
        })}><DatabaseBackup size={16} />{t(language, "nav.backup")}</button>
        <button type="button" onClick={() => void chooseRestore()}><ArchiveRestore size={16} />{t(language, "nav.restore")}</button>
      </div>
    </section>
    <section className="data-card is-danger">
      <h3>{t(language, "data.groupDanger")}</h3>
      <div className="data-menu">
        <button className="is-danger" type="button" disabled={!activeConfig} onClick={() => activeConfig && setConfirmation({ kind: "delete", model: activeConfig })}>
          <Trash2 size={16} />{t(language, "nav.delete")}
        </button>
      </div>
    </section>
  </div>
</div>
```

- [ ] **Step 3: 在 `src/styles/views.css` 追加数据页样式**

```css
/* ===== 数据页（卡片分组） ===== */
.data-page { min-height: 0; flex: 1; display: flex; flex-direction: column; overflow: auto; }
.data-page-body { width: min(100%, 560px); display: grid; gap: var(--space-16); padding: var(--space-24); }
.data-card { border: 1px solid var(--border-subtle); border-radius: var(--radius-10); padding: var(--space-16); background: var(--bg-surface); box-shadow: var(--shadow-1); }
.data-card > h3 { margin: 0 0 var(--space-12); color: var(--gray-11); font-size: var(--text-13); font-weight: var(--weight-semibold); }
.data-card.is-danger { border-color: var(--red-border); background: var(--red-bg); }
.data-card.is-danger > h3 { color: var(--red-text); }
.model-picker { display: grid; gap: 7px; color: var(--gray-10); font-size: var(--text-12); }
.model-picker select { width: 100%; height: 34px; padding: 0 9px; }
.data-menu { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.data-card.is-danger .data-menu { grid-template-columns: 1fr; }
.data-menu button { min-height: 38px; display: flex; align-items: center; gap: 9px; border: 1px solid var(--border-default); border-radius: var(--radius-8); padding: 7px 10px; color: var(--gray-11); background: var(--bg-surface); font-size: var(--text-13); transition: border-color .15s ease, box-shadow .15s ease; }
.data-menu button:hover { border-color: var(--gray-7); box-shadow: var(--shadow-1); }
.data-menu button.is-danger { color: var(--red-text); }
.data-menu button.is-danger:hover { border-color: var(--red-text); background: var(--bg-surface); }
```

- [ ] **Step 4: 从 `src/App.css` 删除已迁移规则**

删除：`.data-page*`、`.model-picker*`、`.data-menu*`（及 `.sidebar .data-menu` 相关残留）。保留：`.modal-*`、`.confirm-*`、`.layout-*`。

- [ ] **Step 5: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（`选择设备型号` select、`导入型号`/`删除型号` 按钮角色不变）

- [ ] **Step 6: Commit**

```bash
git add src/i18n.ts src/App.tsx src/styles/views.css src/App.css
git commit -m "feat: group data page into cards with isolated danger zone"
```

---

### Task 8: 布局编辑器 + 确认弹窗

**Files:**
- Modify: `src/ConfirmDialog.tsx`（标题区图标）
- Modify: `src/LayoutEditor.tsx`（头部副标题）
- Modify: `src/styles/views.css`
- Modify: `src/App.css`（删除全部剩余规则并删除文件）

- [ ] **Step 1: 修改 `src/ConfirmDialog.tsx` 标题区加图标**

第 1 行导入改为：

```tsx
import { Info, TriangleAlert, X } from "lucide-react";
```

`confirm-header` 块（第 30-35 行）改为：

```tsx
<div className="confirm-header">
  <div className={danger ? "confirm-title is-danger" : "confirm-title"}>
    {danger ? <TriangleAlert size={17} /> : <Info size={17} />}
    <h2 id="confirm-title">{title}</h2>
  </div>
  <button className="icon-button" type="button" aria-label={cancelLabel} title={cancelLabel} onClick={onCancel}>
    <X size={17} />
  </button>
</div>
```

- [ ] **Step 2: 修改 `src/LayoutEditor.tsx` 头部加副标题（第 98-101 行）**

```tsx
<div className="layout-editor-header">
  <div>
    <h2 id="layout-editor-title">{t(language, "layout.edit")}</h2>
    <p className="modal-subtitle">{label("修改按键分组、列数与名称", "Edit groups, columns, and button labels")}</p>
  </div>
  <button className="icon-button" type="button" aria-label={t(language, "common.close")} title={t(language, "common.close")} onClick={onCancel}><X size={17} /></button>
</div>
```

- [ ] **Step 3: 在 `src/styles/views.css` 追加弹窗样式**

```css
/* ===== 确认弹窗 ===== */
.modal-backdrop { position: fixed; z-index: 20; inset: 0; display: grid; place-items: center; padding: 20px; background: rgba(18, 25, 22, .42); }
.confirm-dialog { width: min(420px, 100%); border: 1px solid var(--border-default); border-radius: var(--radius-12); background: var(--bg-raised); box-shadow: var(--shadow-3); }
.confirm-header { display: flex; align-items: center; justify-content: space-between; padding: 13px 12px 13px 16px; border-bottom: 1px solid var(--border-subtle); }
.confirm-title { display: flex; align-items: center; gap: 8px; color: var(--gray-11); }
.confirm-title.is-danger { color: var(--red-text); }
.confirm-header h2 { margin: 0; font-size: var(--text-17); font-weight: var(--weight-semibold); color: var(--gray-12); }
.confirm-dialog > p { margin: 16px; color: var(--gray-11); font-size: var(--text-13); line-height: 1.6; }
.confirm-summary { margin: 0 16px 16px; border-left: 3px solid var(--amber-border); border-radius: 0 var(--radius-6) var(--radius-6) 0; padding: 8px 10px; color: var(--amber-text); background: var(--amber-bg); font-size: var(--text-12); }
.confirm-actions { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border-subtle); background: var(--bg-app); border-radius: 0 0 var(--radius-12) var(--radius-12); }

/* ===== 布局编辑器 ===== */
.layout-editor { width: min(860px, calc(100vw - 32px)); max-height: calc(100vh - 32px); border: 1px solid var(--border-strong); border-radius: var(--radius-12); padding: 0; color: inherit; background: var(--bg-raised); box-shadow: var(--shadow-3); }
.layout-editor::backdrop { background: rgba(18, 25, 22, .42); }
.layout-editor-header { display: flex; align-items: center; justify-content: space-between; padding: 13px 14px 13px 18px; border-bottom: 1px solid var(--border-subtle); }
.layout-editor-header h2 { margin: 0; font-size: var(--text-17); font-weight: var(--weight-semibold); }
.modal-subtitle { margin: 2px 0 0; color: var(--gray-9); font-size: var(--text-12); }
.layout-editor-body { max-height: calc(100vh - 155px); overflow: auto; padding: 14px; background: var(--bg-app); }
.layout-group-editor { margin-bottom: 12px; border: 1px solid var(--border-default); border-radius: var(--radius-10); background: var(--bg-surface); }
.layout-group-header, .layout-button-row { display: grid; grid-template-columns: minmax(150px, 1fr) 90px auto; gap: 10px; align-items: end; padding: 10px; border-bottom: 1px solid var(--gray-3); }
.layout-button-row { grid-template-columns: minmax(150px, 1fr) minmax(150px, 1fr) auto; }
.layout-group-header label, .layout-button-row label { display: grid; gap: 5px; color: var(--gray-10); font-size: 10px; }
.layout-group-header input, .layout-button-row input { min-width: 0; height: 32px; padding: 0 8px; }
.layout-add-button { margin: 10px; }
.layout-editor-body > .layout-add-button { margin: 0; }
.layout-editor-footer { display: flex; align-items: center; justify-content: flex-end; gap: 8px; min-height: 58px; padding: 10px 14px; border-top: 1px solid var(--border-subtle); }
.layout-editor-footer > button { min-height: 34px; border: 1px solid var(--border-strong); border-radius: var(--radius-8); padding: 0 12px; background: var(--bg-surface); }
.layout-editor-error { margin: 0 auto 0 0; color: var(--red-text); font-size: var(--text-12); }

@media (max-width: 680px) {
  .layout-group-header, .layout-button-row { grid-template-columns: 1fr; align-items: stretch; }
}
```

- [ ] **Step 4: 删除 `src/App.css` 及 `src/main.tsx` 中的导入**

```bash
git rm src/App.css
```

`src/main.tsx` 删除 `import "./App.css";` 一行。此时 `src/App.css` 应只剩 `.modal-*`、`.confirm-*`、`.layout-*` 规则（已全部迁移）；若仍有其他残留规则，先核对是否已在新文件中覆盖，再删除文件。

- [ ] **Step 5: 运行测试 + 构建**

Run: `npm test && npm run build`
Expected: 全部 PASS（弹窗 `aria-labelledby` 与按钮角色不变）

- [ ] **Step 6: Commit**

```bash
git add src/ConfirmDialog.tsx src/LayoutEditor.tsx src/styles/views.css src/main.tsx src/App.css
git commit -m "feat: refresh modals and remove legacy App.css"
```

---

### Task 9: 全量验证

**Files:** 无（仅验证）

- [ ] **Step 1: 前端测试**

Run: `npm test`
Expected: 全部 PASS

- [ ] **Step 2: 生产构建**

Run: `npm run build`
Expected: `tsc && vite build` 成功，无类型错误

- [ ] **Step 3: Rust 测试（确认未受影响）**

Run: `cd src-tauri && cargo test`
Expected: 全部 PASS

- [ ] **Step 4: 视觉核对**

Run: `npm run dev`，打开 `http://localhost:1420/?preview`，逐视图核对：首页（指标卡/热力图/日志）、按键行为（选中面包屑/键盘/动作面板）、硬件映射（选中行/学习面板折叠态与展开态）、按键布局（打开编辑器模态）、配置文件（三卡片）。截图不可用时请用户目检。
