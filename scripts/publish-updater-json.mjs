#!/usr/bin/env node

import fs from "node:fs";

const repo = process.env.GITHUB_REPOSITORY;
const token = process.env.GITHUB_TOKEN;
const tag = process.env.GITHUB_REF_NAME;

if (!repo || !token || !tag) {
  throw new Error("GITHUB_REPOSITORY, GITHUB_TOKEN and GITHUB_REF_NAME are required");
}

const apiBase = `https://api.github.com/repos/${repo}`;
const headers = {
  "Accept": "application/vnd.github+json",
  "Authorization": `Bearer ${token}`,
  "X-GitHub-Api-Version": "2022-11-28",
};

async function gh(path, options = {}) {
  const res = await fetch(`${apiBase}${path}`, { headers: { ...headers, ...(options.headers || {}) }, ...options });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${options.method || "GET"} ${path} failed: ${res.status} ${body}`);
  }
  return res;
}

async function getJson(path) {
  const res = await gh(path);
  return res.json();
}

function assetUrl(name) {
  return `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name).replaceAll("%2F", "/")}`;
}

async function signatureFor(asset, assets) {
  const sig = assets.find((candidate) => candidate.name === `${asset.name}.sig`);
  if (!sig) throw new Error(`Missing signature asset for ${asset.name}`);
  const res = await fetch(sig.browser_download_url);
  if (!res.ok) throw new Error(`Failed to download ${sig.name}: ${res.status}`);
  return (await res.text()).trim();
}

function findAsset(assets, patterns) {
  return assets.find((asset) => patterns.every((pattern) => pattern.test(asset.name)));
}

async function platformEntry(assets, asset) {
  if (!asset) return null;
  return {
    signature: await signatureFor(asset, assets),
    url: assetUrl(asset.name),
  };
}

function releaseNotes(version) {
  const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## [${version}]`));
  if (start < 0) return `Release ${tag}`;
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## ["));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n").trim() || `Release ${tag}`;
}

const release = await getJson(`/releases/tags/${encodeURIComponent(tag)}`);
const assets = release.assets;
const version = tag.replace(/^v/, "");

const macArchives = assets.filter((asset) => /\.app\.tar\.gz$/.test(asset.name));
const macArm = macArchives.find((asset) => /(aarch64|arm64)/i.test(asset.name)) || (macArchives.length === 1 ? macArchives[0] : null);
const macX64 = macArchives.find((asset) => /(x64|x86_64)/i.test(asset.name)) || (macArchives.length === 1 ? macArchives[0] : null);
const linuxAppImage = findAsset(assets, [/\.AppImage$/]);
const linuxDeb = findAsset(assets, [/\.deb$/]);
const windowsMsi = findAsset(assets, [/\.msi$/]);
const windowsNsis = findAsset(assets, [/(setup|installer).*\.exe$/i]);
const windowsDefault = windowsNsis || windowsMsi;

const platforms = {
  "darwin-aarch64": await platformEntry(assets, macArm),
  "darwin-x86_64": await platformEntry(assets, macX64),
  "linux-x86_64": await platformEntry(assets, linuxAppImage),
  "linux-x86_64-appimage": await platformEntry(assets, linuxAppImage),
  "linux-x86_64-deb": await platformEntry(assets, linuxDeb),
  "windows-x86_64": await platformEntry(assets, windowsDefault),
  "windows-x86_64-nsis": await platformEntry(assets, windowsNsis),
};

if (windowsMsi) {
  platforms["windows-x86_64-msi"] = await platformEntry(assets, windowsMsi);
}

for (const [key, value] of Object.entries(platforms)) {
  if (!value) throw new Error(`Missing updater artifact for ${key}`);
}

const latest = {
  version,
  notes: releaseNotes(version),
  pub_date: release.published_at || new Date().toISOString(),
  platforms,
};

for (const asset of assets.filter((asset) => asset.name === "latest.json")) {
  await gh(`/releases/assets/${asset.id}`, { method: "DELETE" });
}

const body = JSON.stringify(latest, null, 2);
const uploadUrl = release.upload_url.replace(/\{.*$/, "");
const upload = await fetch(`${uploadUrl}?name=latest.json`, {
  method: "POST",
  headers: {
    ...headers,
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(body).toString(),
  },
  body,
});

if (!upload.ok) {
  throw new Error(`Upload latest.json failed: ${upload.status} ${await upload.text()}`);
}

console.log(`Published latest.json for ${Object.keys(platforms).join(", ")}`);
