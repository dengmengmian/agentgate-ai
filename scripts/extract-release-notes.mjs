#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const versionArg = process.argv[2] ?? process.env.GITHUB_REF_NAME ?? "";
const version = versionArg.replace(/^v/, "");

if (!version) {
  console.error("Usage: node scripts/extract-release-notes.mjs <version>");
  process.exit(1);
}

const curatedPath = path.join("docs", "release-notes", `${version}.md`);
if (fs.existsSync(curatedPath)) {
  process.stdout.write(fs.readFileSync(curatedPath, "utf8").trimEnd() + "\n");
  process.exit(0);
}

const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
const header = new RegExp(`^## \\[${escapeRegExp(version)}\\][^\\n]*\\n`, "m");
const match = changelog.match(header);

if (!match || match.index === undefined) {
  process.stdout.write(`## AgentGate ${version}\n\nRelease ${version}.\n`);
  process.exit(0);
}

const start = match.index + match[0].length;
const rest = changelog.slice(start);
const next = rest.search(/^## \[/m);
const body = (next >= 0 ? rest.slice(0, next) : rest).trim();

const bilingualBody = body
  .replace(/^### 新增$/gm, "### Added / 新增")
  .replace(/^### 改进$/gm, "### Improvements / 改进")
  .replace(/^### 修复$/gm, "### Fixes / 修复")
  .replace(/^### 安全$/gm, "### Security / 安全")
  .replace(/^### 性能$/gm, "### Performance / 性能")
  .replace(/^### 文档$/gm, "### Documentation / 文档")
  .replace(/^### 重命名$/gm, "### Renamed / 重命名");

process.stdout.write(`## AgentGate ${version}\n\n`);
process.stdout.write("> English section headings are generated automatically. Detailed notes follow the bilingual changelog source.\n\n");
process.stdout.write(`${bilingualBody}\n`);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
