# Homebrew Cask 发布

`agentgate.rb` 是 AgentGate 的 Homebrew cask 定义，通过自有 tap 分发（不需要进 homebrew-core）。

## 首次建 tap（一次性）

1. 在 GitHub 创建公开仓库 `dengmengmian/homebrew-tap`。
2. 把本目录的 `agentgate.rb` 复制到该仓库的 `Casks/agentgate.rb` 并推送。
3. 用户即可安装：

```bash
brew tap dengmengmian/tap
brew install --cask agentgate
```

安装命令建议同步写进 README.md / README_ZH.md 的 Download 区块和官网下载区。

## 每次发版更新

发布新版本后更新 `version` 和两个 `sha256`，同步到 tap 仓库：

```bash
VERSION=1.5.0
curl -sLO "https://github.com/dengmengmian/agentgate-ai/releases/download/v${VERSION}/AgentGate_${VERSION}_aarch64.dmg"
curl -sLO "https://github.com/dengmengmian/agentgate-ai/releases/download/v${VERSION}/AgentGate_${VERSION}_x64.dmg"
shasum -a 256 AgentGate_${VERSION}_*.dmg
```

后续可在 release.yml 里加一步：发版时自动改写 tap 仓库的 cask（用 PAT push），当前先手动。

## 本地验证

```bash
brew style --cask packaging/homebrew/agentgate.rb
brew install --cask packaging/homebrew/agentgate.rb
```
