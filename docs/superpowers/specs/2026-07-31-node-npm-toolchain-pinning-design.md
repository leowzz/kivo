# Kivo Node/npm 工具链版本设计

日期：2026-07-31
状态：已获用户批准

## 目标

GitHub Actions 和维护者使用可复现的参考工具链：

- Node.js `24.18.0`
- npm `11.16.0`

开源贡献者允许使用兼容的同主版本工具链：

- Node.js `>=24.12.0 <25`
- npm `>=11.0.0 <12`

这样既避免不同主版本反复重写 `package-lock.json`，也不要求每个贡献者安装完全相同的补丁版本。

## 版本来源

- `.nvmrc` 使用参考版本 `24.18.0`，供 nvm、direnv 和 CI 使用。
- `package.json` 新增 `packageManager: "npm@11.16.0"`。
- 标准 `engines` 字段向 npm、其他包管理器和 IDE 声明受支持的兼容范围。
- `package.json` 的 `devEngines` 对 Node `>=24.12.0 <25` 和 npm
  `>=11.0.0 <12` 使用 `onFail: "error"`，只阻止未经支持的主版本或过旧版本。
- 测试依赖使用 `jsdom@29.1.1`；jsdom 30 要求 Node 24.15 以上，会无必要地排除已验证可用的 Node 24.12。

不增加额外版本管理器或版本检查脚本。

## 本地 direnv/nvm 集成

新增 `.envrc`：

1. 监听 `.nvmrc`，版本文件变化时让 direnv 重新加载。
2. 从现有 `NVM_DIR` 加载 `nvm.sh`。
3. 执行静默的 `nvm use`，进入仓库时自动切换到 `.nvmrc` 指定版本。
4. nvm 未安装、`NVM_DIR` 不可用或目标 Node 版本缺失时明确失败，不自动联网安装。

首次启用需要开发者执行一次 `direnv allow`。

## CI 对齐

`.github/workflows/release-windows.yml` 的 `actions/setup-node` 使用
`node-version-file: .nvmrc`，使 CI 始终使用参考版本；本地贡献者可以使用声明范围内的版本。

CI 保持使用 `npm ci` 和现有 npm 缓存配置。

## Lockfile 处理

在 Node `24.18.0`、npm `11.16.0` 环境中运行 `npm install --package-lock-only`，让根包的工具链元数据写入 `package-lock.json`。连续运行第二次必须不再产生差异，证明锁文件生成稳定。

不升级任何应用依赖，不接受与工具链元数据无关的 lockfile 变化。

## 验证

1. `direnv exec . node --version` 输出 `v24.18.0`。
2. `direnv exec . npm --version` 输出 `11.16.0`。
3. Node `24.12.0` 以上的 24.x 与兼容的 npm 11 也能执行 `npm run`。
4. 固定环境连续两次执行 `npm install --package-lock-only`，第二次无 Git diff。
5. `npm test` 与 `npm run build` 通过。
6. `git diff --check` 通过。
