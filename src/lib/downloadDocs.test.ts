import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const packageJson = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
) as { version: string };

const downloadDocs = [
  "README.md",
  "README_ZH.md",
  "docs/full-reference.md",
  "docs/full-reference-zh.md",
];

describe("download documentation", () => {
  it.each(downloadDocs)("uses the package version in %s", (file) => {
    const content = fs.readFileSync(path.join(root, file), "utf8");

    const releaseVersions = [
      ...content.matchAll(
        /github\.com\/dengmengmian\/agentgate-ai\/releases\/download\/v([0-9]+\.[0-9]+\.[0-9]+)/g,
      ),
    ].map((match) => match[1]);
    const assetVersions = [
      ...content.matchAll(/AgentGate_([0-9]+\.[0-9]+\.[0-9]+)_/g),
    ].map((match) => match[1]);

    expect(releaseVersions.length).toBeGreaterThan(0);
    expect(assetVersions.length).toBeGreaterThan(0);
    expect(new Set(releaseVersions)).toEqual(new Set([packageJson.version]));
    expect(new Set(assetVersions)).toEqual(new Set([packageJson.version]));
  });
});
