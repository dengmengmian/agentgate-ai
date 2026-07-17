cask "agentgate" do
  arch arm: "aarch64", intel: "x64"

  version "1.5.1"
  sha256 arm:   "1ace38c6ec16152e47ee01f69a44bf3124506d6e4eb667f25b20411e4bbf9bea",
         intel: "e112f08a138bce0bce2f7ece16fc3a82e84d1bf9fc7199cd5b6cd99891d7f7ec"

  url "https://github.com/dengmengmian/agentgate-ai/releases/download/v#{version}/AgentGate_#{version}_#{arch}.dmg",
      verified: "github.com/dengmengmian/agentgate-ai/"
  name "AgentGate"
  desc "Local AI gateway that routes, converts, fails over, and traces model requests"
  homepage "https://dengmengmian.github.io/agentgate-ai/"

  livecheck do
    url :url
    strategy :github_latest
  end

  # 应用内置 tauri-plugin-updater 自更新
  auto_updates true
  depends_on :macos

  app "AgentGate.app"

  zap trash: [
    "~/Library/Application Support/com.mengmian.agentgate",
    "~/Library/Caches/com.mengmian.agentgate",
    "~/Library/Preferences/com.mengmian.agentgate.plist",
    "~/Library/Saved Application State/com.mengmian.agentgate.savedState",
    "~/Library/WebKit/com.mengmian.agentgate",
  ]
end
