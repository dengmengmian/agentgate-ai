// Dynamic download table from GitHub Releases API.
//
// Strategy:
//   - HTML ships a static fallback (current pinned version) so users can
//     always download even with no JS / no network / API rate-limited.
//   - On load, fetch the latest release, then atomically replace tbody with
//     dynamic rows (live version, filenames, sizes, "recommended" highlight).
//   - localStorage cache 1h to avoid hitting GitHub's 60-req/hr unauthenticated
//     rate limit on repeat visits.
//   - i18n via document.documentElement.lang (en | zh).

(function () {
  const REPO = "dengmengmian/agentgate-ai";
  const CACHE_KEY = "agentgate-release-cache-v1";
  const CACHE_TTL_MS = 60 * 60 * 1000;

  // Asset filename → platform. Build matcher list once.
  // Filenames look like:
  //   AgentGate_1.4.1_aarch64.dmg
  //   AgentGate_1.4.1_x64.dmg
  //   AgentGate_1.4.1_x64-setup.exe
  //   AgentGate_1.4.1_amd64.deb
  //   AgentGate_1.4.1_amd64.AppImage
  const PLATFORMS = [
    { id: "mac-arm", match: /aarch64\.dmg$/i },
    { id: "mac-x86", match: /_x64\.dmg$/i },
    { id: "win", match: /x64-setup\.exe$/i },
    { id: "linux-deb", match: /\.deb$/i },
    { id: "linux-appimage", match: /\.AppImage$/i },
  ];

  const LABELS = {
    en: {
      platforms: {
        "mac-arm": { os: "macOS", detail: "Apple Silicon" },
        "mac-x86": { os: "macOS", detail: "Intel" },
        win: { os: "Windows", detail: "10 / 11" },
        "linux-deb": { os: "Linux", detail: "Debian / Ubuntu" },
        "linux-appimage": { os: "Linux", detail: "other distros" },
      },
      recommended: "recommended",
      openReleases: "open releases",
      today: "today",
      yesterday: "yesterday",
      daysAgo: (n) => `${n} days ago`,
      monthsAgo: (n) => `${n} months ago`,
      yearsAgo: (n) => `${n} years ago`,
    },
    zh: {
      platforms: {
        "mac-arm": { os: "macOS", detail: "Apple 芯片" },
        "mac-x86": { os: "macOS", detail: "Intel 芯片" },
        win: { os: "Windows", detail: "10 / 11" },
        "linux-deb": { os: "Linux", detail: "Debian / Ubuntu" },
        "linux-appimage": { os: "Linux", detail: "其他发行版" },
      },
      recommended: "推荐",
      openReleases: "打开 releases",
      today: "今天",
      yesterday: "昨天",
      daysAgo: (n) => `${n} 天前`,
      monthsAgo: (n) => `${n} 个月前`,
      yearsAgo: (n) => `${n} 年前`,
    },
  };

  const lang = (document.documentElement.lang || "en")
    .toLowerCase()
    .startsWith("zh")
    ? "zh"
    : "en";
  const L = LABELS[lang];

  // Browser UA → platform id. Best-effort, user can still pick anything.
  // mac arm vs intel: navigator.userAgentData.getHighEntropyValues({architecture})
  // is Chromium-only and async; we default macOS to Apple Silicon since M-series
  // is the dominant device after 5 years. Intel users click the row below.
  function detectPlatformId() {
    const ua = navigator.userAgent || "";
    if (/Win(dows|32|64)/i.test(ua)) return "win";
    if (/Mac OS X|Macintosh/i.test(ua)) return "mac-arm";
    if (/Linux/i.test(ua)) return "linux-deb";
    return null;
  }

  function readCache() {
    try {
      const raw = localStorage.getItem(CACHE_KEY);
      if (!raw) return null;
      const { ts, data } = JSON.parse(raw);
      if (Date.now() - ts > CACHE_TTL_MS) return null;
      return data;
    } catch {
      return null;
    }
  }

  function writeCache(data) {
    try {
      localStorage.setItem(
        CACHE_KEY,
        JSON.stringify({ ts: Date.now(), data })
      );
    } catch {
      // quota exceeded / private mode — silently skip
    }
  }

  async function loadLatestRelease() {
    const cached = readCache();
    if (cached) return cached;
    const resp = await fetch(
      `https://api.github.com/repos/${REPO}/releases/latest`,
      { headers: { Accept: "application/vnd.github+json" } }
    );
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const data = await resp.json();
    writeCache(data);
    return data;
  }

  function formatBytes(n) {
    if (!n || n < 1024) return `${n || 0}B`;
    const mb = n / (1024 * 1024);
    if (mb >= 1) return `${mb.toFixed(1)}MB`;
    return `${Math.round(n / 1024)}KB`;
  }

  function formatRelativeTime(iso) {
    if (!iso) return "";
    const diff = Date.now() - new Date(iso).getTime();
    const day = 24 * 60 * 60 * 1000;
    const days = Math.floor(diff / day);
    if (days < 1) return L.today;
    if (days < 2) return L.yesterday;
    if (days < 30) return L.daysAgo(days);
    if (days < 365) return L.monthsAgo(Math.floor(days / 30));
    return L.yearsAgo(Math.floor(days / 365));
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function renderRow(platform, asset, recommended) {
    const labels = L.platforms[platform.id];
    const url =
      asset?.browser_download_url ||
      `https://github.com/${REPO}/releases/latest`;
    const filename = asset?.name || L.openReleases;
    const size = asset?.size
      ? `<span class="ml-2 text-faint">${formatBytes(asset.size)}</span>`
      : "";
    const rowCls = recommended ? "bg-amber/5" : "";
    const arrow = recommended
      ? '<span class="text-amber">▸</span>'
      : '<span class="text-faint">▸</span>';
    const tag = recommended
      ? `<span class="ml-2 text-[10px] uppercase tracking-wider text-amber">${L.recommended}</span>`
      : "";
    return `
      <tr class="border-b border-border-soft ${rowCls}">
        <td class="px-5 py-4">
          ${arrow}
          <span class="ml-2 text-ink">${escapeHtml(labels.os)}</span>
          <span class="ml-1 text-muted">${escapeHtml(labels.detail)}</span>
          ${tag}
        </td>
        <td class="px-5 py-4 text-right">
          <a class="text-cyan hover:text-amber" href="${escapeHtml(url)}">
            ${escapeHtml(filename)} ${size} <span class="text-faint">↗</span>
          </a>
        </td>
      </tr>
    `;
  }

  function render(release, platformId) {
    const tbody = document.querySelector("[data-downloads]");
    if (!tbody) return;

    // querySelectorAll: nav / hero / 下载区都有 [data-version],
    // 一次拉到 latest release 后所有显示位置同步更新,下次发版不用手改。
    const versions = document.querySelectorAll("[data-version]");
    const released = document.querySelector("[data-released]");
    if (release?.tag_name) {
      versions.forEach((el) => {
        el.textContent = release.tag_name;
      });
    }
    if (released && release?.published_at) {
      released.textContent = formatRelativeTime(release.published_at);
    }

    const assets = release?.assets || [];
    const rowsHtml = PLATFORMS.map((p) => {
      const asset = assets.find((a) => p.match.test(a.name));
      return renderRow(p, asset, p.id === platformId);
    }).join("");

    tbody.innerHTML = rowsHtml;
  }

  async function init() {
    const platformId = detectPlatformId();
    try {
      const release = await loadLatestRelease();
      render(release, platformId);
    } catch (e) {
      // Leave HTML default rows in place. Log for debugging only.
      console.warn("[agentgate] failed to load releases:", e);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
