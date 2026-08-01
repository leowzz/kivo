# Kivo Node/npm 工具链精确锁定设计

日期：2026-07-31
状态：已获用户批准

## 目标

将本地开发和 GitHub Actions 使用的 JavaScript 工具链精确锁定为：

- Node.js `24.18.0`
- npm `11.16.0`

避免不同 npm 主版本反复重写 `package-lock.json`，尤其是丢失 Linux 原生可选依赖的 `libc` 元数据。

## 版本来源

- 新增 `.nvmrc`，内容为精确版本 `24.18.0`，作为 Node 版本的唯一来源。
- `package.json` 新增 `packageManager: "npm@11.16.0"`。
- `package.json` 新增 `devEngines`，对 Node `24.18.0` 和 npm `11.16.0` 使用 `onFail: "error"`，使 `npm install`、`npm ci` 和 `npm run` 在版本不匹配时直接失败。

不增加额外版本管理器或版本检查脚本。

## 本地 direnv/nvm 集成

新增 `.envrc`：

1. 监听 `.nvmrc`，版本文件变化时让 direnv 重新加载。
2. 从现有 `NVM_DIR` 加载 `nvm.sh`。
3. 执行静默的 `nvm use`，进入仓库时自动切换到 `.nvmrc` 指定版本。
4. nvm 未安装、`NVM_DIR` 不可用或目标 Node 版本缺失时明确失败，不自动联网安装。

首次启用需要开发者执行一次 `direnv allow`。

## CI 对齐

将 `.github/workflows/release-windows.yml` 的 `actions/setup-node` 配置从浮动的 `node-version: 24` 改为 `node-version-file: .nvmrc`。后续 Node 升级只修改 `.nvmrc`，本地和 CI 同步生效。

CI 保持使用 `npm ci` 和现有 npm 缓存配置。

## Lockfile 处理

在 Node `24.18.0`、npm `11.16.0` 环境中运行 `npm install --package-lock-only`，让根包的工具链元数据写入 `package-lock.json`。连续运行第二次必须不再产生差异，证明锁文件生成稳定。

不升级任何应用依赖，不接受与工具链元数据无关的 lockfile 变化。

## 验证

1. `direnv exec . node --version` 输出 `v24.18.0`。
2. `direnv exec . npm --version` 输出 `11.16.0`。
3. 固定环境连续两次执行 `npm install --package-lock-only`，第二次无 Git diff。
4. `npm test` 与 `npm run build` 通过。
5. `git diff --check` 通过，最终 diff 只包含版本配置、CI 对齐和对应 lockfile 元数据。
