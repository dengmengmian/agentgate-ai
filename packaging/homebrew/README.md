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

release.yml 的 `homebrew-cask` job 会在发版后自动更新 tap 仓库的 cask（version + 两个 dmg 的 sha256），
需要仓库 Secret `HOMEBREW_TAP_TOKEN`（对 dengmengmian/homebrew-tap 有 contents:write 的 fine-grained PAT）。
本目录的 `agentgate.rb` 是模板/参考副本，结构改动时需与 tap 仓库同步。

## 本地验证

```bash
brew style --cask packaging/homebrew/agentgate.rb
brew install --cask packaging/homebrew/agentgate.rb
```
