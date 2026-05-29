#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const catalogDir = path.join(root, "provider-catalog", "providers");
const args = process.argv.slice(2);
const check = args.includes("--check");
const update = args.includes("--update");
const strict = args.includes("--strict");

const providerFilter = readArgValue("--provider");

function readArgValue(name) {
  const inline = args.find((arg) => arg.startsWith(`${name}=`));
  if (inline) return inline.slice(name.length + 1);
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : null;
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function loadProviders() {
  return fs.readdirSync(catalogDir)
    .filter((file) => file.endsWith(".json"))
    .sort()
    .map((file) => ({
      file,
      path: path.join(catalogDir, file),
      provider: readJson(path.join(catalogDir, file)),
    }))
    .filter(({ provider }) => !providerFilter || provider.type === providerFilter);
}

function keyForSync(sync) {
  const envVars = Array.isArray(sync.envVar) ? sync.envVar : [sync.envVar];
  for (const envVar of envVars) {
    const value = process.env[envVar]?.trim();
    if (value) return { envVar, value };
  }
  return { envVar: envVars.filter(Boolean).join(" | "), value: "" };
}

function requestForSync(sync, key) {
  const url = new URL(sync.modelsUrl);
  const headers = {
    accept: "application/json",
    ...(sync.headers ?? {}),
  };

  switch (sync.authHeader) {
    case "bearer":
      headers.authorization = `Bearer ${key}`;
      break;
    case "x-api-key":
      headers["x-api-key"] = key;
      break;
    case "query:key":
      url.searchParams.set("key", key);
      break;
    case undefined:
    case "none":
      break;
    default:
      if (sync.authHeader.startsWith("header:")) {
        headers[sync.authHeader.slice("header:".length)] = key;
        break;
      }
      throw new Error(`Unsupported authHeader: ${sync.authHeader}`);
  }

  return { url, headers };
}

async function fetchJson(sync, key) {
  const { url, headers } = requestForSync(sync, key);
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 20_000);
  try {
    const response = await fetch(url, { headers, signal: controller.signal });
    const text = await response.text();
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${text.slice(0, 500)}`);
    }
    return text ? JSON.parse(text) : {};
  } finally {
    clearTimeout(timeout);
  }
}

function asArray(payload) {
  if (Array.isArray(payload)) return payload;
  if (Array.isArray(payload.data)) return payload.data;
  if (Array.isArray(payload.models)) return payload.models;
  if (Array.isArray(payload.items)) return payload.items;
  return [];
}

function modelIdFromItem(item) {
  if (typeof item === "string") return item;
  if (!item || typeof item !== "object") return "";
  return item.id ?? item.name ?? item.model ?? item.model_id ?? "";
}

function compilePatterns(value) {
  const patterns = Array.isArray(value) ? value : value ? [value] : [];
  return patterns.map((pattern) => new RegExp(pattern));
}

function normalizeRemoteModels(payload, sync) {
  const include = compilePatterns(sync.includeRegex);
  const exclude = compilePatterns(sync.excludeRegex);
  const ids = asArray(payload)
    .map(modelIdFromItem)
    .filter((id) => typeof id === "string" && id.trim())
    .map((id) => id.trim())
    .map((id) => sync.idPrefixToStrip && id.startsWith(sync.idPrefixToStrip)
      ? id.slice(sync.idPrefixToStrip.length)
      : id)
    .filter((id) => include.length === 0 || include.some((pattern) => pattern.test(id)))
    .filter((id) => !exclude.some((pattern) => pattern.test(id)));
  return [...new Set(ids)].sort((a, b) => a.localeCompare(b));
}

function catalogModels(provider) {
  if (provider.supportedModels?.length) {
    return [...new Set(provider.supportedModels)].sort((a, b) => a.localeCompare(b));
  }
  return [...new Set((provider.models ?? [])
    .map((model) => model.id)
    .filter((id) => id && id !== "*"))]
    .sort((a, b) => a.localeCompare(b));
}

function diffModels(catalog, remote) {
  const catalogSet = new Set(catalog);
  const remoteSet = new Set(remote);
  return {
    missingRemote: catalog.filter((id) => !remoteSet.has(id)),
    newRemote: remote.filter((id) => !catalogSet.has(id)),
  };
}

function applyRemoteModels(provider, remoteModels) {
  provider.supportedModels = remoteModels;
  const existing = new Set((provider.models ?? []).map((model) => model.id));
  const additions = remoteModels
    .filter((id) => !existing.has(id))
    .map((id) => ({ id }));
  if (additions.length > 0) {
    provider.models = [...(provider.models ?? []), ...additions];
  }
}

function formatList(values) {
  if (values.length === 0) return "-";
  return values.slice(0, 12).join(", ") + (values.length > 12 ? `, ... (+${values.length - 12})` : "");
}

async function main() {
  const providers = loadProviders();
  if (providerFilter && providers.length === 0) {
    throw new Error(`No provider catalog entry matched --provider=${providerFilter}`);
  }

  const results = [];
  let failed = false;

  for (const entry of providers) {
    const { provider } = entry;
    if (!provider.sync) {
      results.push({ type: provider.type, status: "no-sync" });
      continue;
    }

    const key = keyForSync(provider.sync);
    if (!key.value) {
      results.push({ type: provider.type, status: "skipped", reason: `missing ${key.envVar}` });
      continue;
    }

    try {
      const payload = await fetchJson(provider.sync, key.value);
      const remote = normalizeRemoteModels(payload, provider.sync);
      const catalog = catalogModels(provider);
      const diff = diffModels(catalog, remote);
      const shouldFail = diff.missingRemote.length > 0 || (strict && diff.newRemote.length > 0);
      failed = failed || (check && shouldFail);

      if (update && remote.length > 0) {
        applyRemoteModels(provider, remote);
        writeJson(entry.path, provider);
      }

      results.push({
        type: provider.type,
        status: shouldFail ? "drift" : "ok",
        catalogCount: catalog.length,
        remoteCount: remote.length,
        missingRemote: diff.missingRemote,
        newRemote: diff.newRemote,
        updated: update,
      });
    } catch (error) {
      failed = true;
      results.push({ type: provider.type, status: "error", reason: error.message });
    }
  }

  console.log("Provider catalog sync report:");
  for (const result of results) {
    if (result.status === "ok" || result.status === "drift") {
      console.log(`  - ${result.type}: ${result.status} (catalog ${result.catalogCount}, remote ${result.remoteCount})`);
      if (result.missingRemote.length > 0) console.log(`    missing upstream: ${formatList(result.missingRemote)}`);
      if (result.newRemote.length > 0) console.log(`    new upstream: ${formatList(result.newRemote)}`);
      if (result.updated) console.log("    catalog file updated from upstream model ids");
    } else {
      console.log(`  - ${result.type}: ${result.status}${result.reason ? ` (${result.reason})` : ""}`);
    }
  }

  if (failed) {
    process.exitCode = 1;
  }
}

await main();
