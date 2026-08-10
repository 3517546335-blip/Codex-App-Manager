#!/usr/bin/env node
// For fork releases, build the app with the updater pointed only at the public
// release repository. This keeps the source repo private while leaving updater
// metadata and installers reachable to normal users without GitHub auth.
import { readFileSync, writeFileSync } from "node:fs";

const repo = process.env.GITHUB_REPOSITORY;
const upstream = "Wangnov/Codex-App-Manager";
if (!repo || repo === upstream) {
  console.log("[release-updater] upstream repo or local build; leaving updater endpoints unchanged");
  process.exit(0);
}
const publicReleaseRepo =
  process.env.MANAGER_PUBLIC_RELEASE_REPOSITORY ||
  (repo === "q4549286/Codex-App-Manager" ? "q4549286/Codex-Desktop-Releases" : repo);

const configPath = "src-tauri/tauri.conf.json";
const config = JSON.parse(readFileSync(configPath, "utf8"));
const updater = config.plugins?.updater;
if (!updater || !Array.isArray(updater.endpoints)) {
  throw new Error("plugins.updater.endpoints is missing from tauri.conf.json");
}

const forkEndpoint = `https://github.com/${publicReleaseRepo}/releases/latest/download/latest.json`;
updater.endpoints = [forkEndpoint];

writeFileSync(configPath, JSON.stringify(config, null, 2) + "\n");
console.log(`[release-updater] updater GitHub endpoint set to ${forkEndpoint}`);
