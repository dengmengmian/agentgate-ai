import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const packageJson = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
);

const downloadDocs = [
  "README.md",
  "README_ZH.md",
  "docs/full-reference.md",
  "docs/full-reference-zh.md",
];

let failed = false;

for (const file of downloadDocs) {
  const content = fs.readFileSync(path.join(root, file), "utf8");
  const releaseVersions = [
    ...content.matchAll(
      /github\.com\/dengmengmian\/agentgate-ai\/releases\/download\/v([0-9]+\.[0-9]+\.[0-9]+)/g,
    ),
  ].map((match) => match[1]);
  const assetVersions = [
    ...content.matchAll(/AgentGate_([0-9]+\.[0-9]+\.[0-9]+)_/g),
  ].map((match) => match[1]);

  const versions = new Set([...releaseVersions, ...assetVersions]);
  if (versions.size !== 1 || !versions.has(packageJson.version)) {
    failed = true;
    console.error(
      `${file} download version mismatch: expected ${packageJson.version}, found ${[...versions].join(", ") || "none"}`,
    );
  }
}

if (failed) {
  process.exitCode = 1;
}
