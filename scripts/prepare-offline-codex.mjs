#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { mkdir, rename, rm, stat, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { pathToFileURL } from "node:url";

import { XMLParser } from "fast-xml-parser";

const WINDOWS_DESCRIPTOR =
  "https://q4549286.github.io/Codex-Desktop-Releases/official-source/windows-x64.json";
const MAC_APPCASTS = {
  arm64: "https://persistent.oaistatic.com/codex-app-prod/appcast.xml",
  x86_64: "https://persistent.oaistatic.com/codex-app-prod/appcast-x64.xml",
};
const ROOT = join("src-tauri", "resources", "offline");
const MAX_PACKAGE_BYTES = 2 * 1024 * 1024 * 1024;

function requireValue(condition, message) {
  if (!condition) throw new Error(message);
}

async function fetchOk(url) {
  const response = await fetch(url, {
    redirect: "follow",
    headers: { "user-agent": "Codex-Desktop-Manager-offline-builder" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status} for ${url}`);
  return response;
}

async function downloadVerified(url, destination, expectedLength) {
  requireValue(Number.isSafeInteger(expectedLength) && expectedLength > 0, "invalid package length");
  requireValue(expectedLength <= MAX_PACKAGE_BYTES, "package exceeds offline bundle size limit");
  await mkdir(dirname(destination), { recursive: true });
  const temporary = `${destination}.part`;
  await rm(temporary, { force: true });

  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetchOk(url);
      requireValue(response.body, `empty response body for ${url}`);
      const hash = createHash("sha256");
      let downloaded = 0;
      const meter = new Transform({
        transform(chunk, _encoding, callback) {
          downloaded += chunk.length;
          if (downloaded > MAX_PACKAGE_BYTES) {
            callback(new Error("download exceeded offline bundle size limit"));
            return;
          }
          hash.update(chunk);
          callback(null, chunk);
        },
      });
      await pipeline(
        Readable.fromWeb(response.body),
        meter,
        createWriteStream(temporary, { flags: "w" }),
      );
      requireValue(downloaded === expectedLength, `package length mismatch: ${downloaded} != ${expectedLength}`);
      await rm(destination, { force: true });
      await rename(temporary, destination);
      return { bytes: downloaded, sha256: hash.digest("hex") };
    } catch (error) {
      lastError = error;
      await rm(temporary, { force: true });
      if (attempt < 3) await new Promise((resolve) => setTimeout(resolve, 10_000));
    }
  }
  throw lastError;
}

export function validateWindowsDescriptor(source) {
  requireValue(source?.schemaVersion === 1, "unsupported Windows descriptor schema");
  requireValue(source.storeProductId === "9PLM9XGG6VKS", "unexpected Windows Store product ID");
  requireValue(source.packageIdentity === "OpenAI.Codex", "unexpected Windows package identity");
  requireValue(source.architecture === "x64", "Windows offline package must be x64");
  requireValue(typeof source.version === "string" && source.version, "missing Codex version");
  requireValue(typeof source.packageVersion === "string" && source.packageVersion, "missing package version");
  requireValue(typeof source.packageMoniker === "string" && source.packageMoniker, "missing package moniker");
  requireValue(Number.isSafeInteger(source.contentLength) && source.contentLength > 0, "invalid package length");

  const url = new URL(source.url);
  const host = url.hostname.toLowerCase();
  requireValue(url.protocol === "http:" || url.protocol === "https:", "unexpected Microsoft URL protocol");
  requireValue(host === "dl.delivery.mp.microsoft.com" || host.endsWith(".dl.delivery.mp.microsoft.com"), "unexpected Microsoft download host");
  requireValue(url.pathname.startsWith("/filestreamingservice/files/"), "unexpected Microsoft download path");
  const expiresAt = Number(url.searchParams.get("P1"));
  requireValue(Number.isFinite(expiresAt) && expiresAt * 1000 > Date.now() + 60_000, "Microsoft package URL is expired");
}

async function prepareWindows() {
  const destination = join(ROOT, "windows", "latest");
  await rm(ROOT, { recursive: true, force: true });
  await mkdir(destination, { recursive: true });

  const source = await (await fetchOk(WINDOWS_DESCRIPTOR)).json();
  validateWindowsDescriptor(source);
  const packagePath = join(destination, "win-x64");
  const downloaded = await downloadVerified(source.url, packagePath, source.contentLength);
  const architecture = {
    version: source.packageVersion,
    appVersion: source.version,
    packageMoniker: source.packageMoniker,
    architecture: "x64",
    contentLength: source.contentLength,
    lastModified: source.generatedAt ?? null,
    downloadable: true,
  };
  const manifest = {
    schemaVersion: 2,
    codexVersion: source.version,
    publishedAt: source.generatedAt ?? new Date().toISOString(),
    sources: {
      windows: {
        ...architecture,
        productId: source.storeProductId,
        updateManifest: {
          storeProductId: source.storeProductId,
          packageIdentity: source.packageIdentity,
        },
        architectures: { x64: architecture },
      },
    },
  };
  await writeFile(join(destination, "manifest"), `${JSON.stringify(manifest, null, 2)}\n`);
  await writeFile(
    join(destination, "checksums"),
    `${downloaded.sha256}  ${source.packageMoniker}.Msix\n`,
  );
  console.log(
    `[offline] Windows x64 ${source.version}: ${downloaded.bytes} bytes, sha256=${downloaded.sha256}`,
  );
}

function first(value) {
  return Array.isArray(value) ? value[0] : value;
}

export function parseLatestMacItem(xml) {
  const parser = new XMLParser({
    ignoreAttributes: false,
    attributeNamePrefix: "",
    removeNSPrefix: true,
    parseTagValue: false,
    parseAttributeValue: false,
  });
  const parsed = parser.parse(xml);
  const rawItems = parsed?.rss?.channel?.item;
  const items = (Array.isArray(rawItems) ? rawItems : [rawItems]).filter(Boolean);
  const normalized = items.map((item) => {
    const enclosure = first(item.enclosure);
    return {
      build: Number(item.version),
      shortVersion: String(item.shortVersionString ?? item.title ?? "").trim(),
      minimumSystemVersion: item.minimumSystemVersion
        ? String(item.minimumSystemVersion).trim()
        : null,
      pubDate: item.pubDate ? String(item.pubDate).trim() : null,
      url: String(enclosure?.url ?? ""),
      contentLength: Number(enclosure?.length),
      edSignature: String(enclosure?.edSignature ?? "").trim(),
    };
  });
  normalized.sort((a, b) => b.build - a.build);
  const latest = normalized[0];
  requireValue(Number.isSafeInteger(latest?.build) && latest.build > 0, "invalid latest macOS build");
  requireValue(latest.shortVersion, "missing macOS short version");
  requireValue(Number.isSafeInteger(latest.contentLength) && latest.contentLength > 0, "invalid macOS package length");
  requireValue(new URL(latest.url).protocol === "https:", "macOS package URL must use HTTPS");
  requireValue(Buffer.from(latest.edSignature, "base64").length === 64, "invalid Sparkle EdDSA signature");
  return latest;
}

async function prepareMac(target) {
  const architecture = target.startsWith("aarch64-") ? "arm64" : target.startsWith("x86_64-") ? "x86_64" : null;
  requireValue(architecture, `unsupported macOS target: ${target}`);
  const destination = join(ROOT, "macos");
  await rm(ROOT, { recursive: true, force: true });
  await mkdir(destination, { recursive: true });

  const appcastUrl = MAC_APPCASTS[architecture];
  const xml = await (await fetchOk(appcastUrl)).text();
  const latest = parseLatestMacItem(xml);
  const packageFile = basename(new URL(latest.url).pathname) || "ChatGPT.zip";
  const packagePath = join(destination, packageFile);
  const downloaded = await downloadVerified(latest.url, packagePath, latest.contentLength);
  const metadata = {
    architecture,
    build: latest.build,
    shortVersion: latest.shortVersion,
    minimumSystemVersion: latest.minimumSystemVersion,
    pubDate: latest.pubDate,
    packageFile,
    contentLength: latest.contentLength,
    edSignature: latest.edSignature,
  };
  await writeFile(join(destination, "metadata.json"), `${JSON.stringify(metadata, null, 2)}\n`);
  const actual = await stat(packagePath);
  requireValue(actual.size === latest.contentLength, "macOS package changed after download");
  console.log(
    `[offline] macOS ${architecture} ${latest.shortVersion}: ${downloaded.bytes} bytes, sha256=${downloaded.sha256}`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [, , platform, target = ""] = process.argv;
  if (platform === "windows") {
    await prepareWindows();
  } else if (platform === "macos") {
    await prepareMac(target);
  } else {
    throw new Error("usage: prepare-offline-codex.mjs <windows|macos> <rust-target>");
  }
}
