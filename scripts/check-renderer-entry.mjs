import { readFileSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const dist = new URL("../dist/", import.meta.url);
const html = readFileSync(new URL("index.html", dist), "utf8");
const entryMatch = html.match(/<script[^>]+type="module"[^>]+src="([^"]+)"/);
if (!entryMatch) throw new Error("renderer entry script was not found in dist/index.html");

const entryName = basename(entryMatch[1]);
const assetsDir = new URL("assets/", dist);
const assetsPath = fileURLToPath(assetsDir);
const entry = readFileSync(new URL(entryName, assetsDir), "utf8");

const forbiddenEntryMarkers = [
  "react.transitional.element",
  "__REACT_DEVTOOLS_GLOBAL_HOOK__",
  "createRoot=function",
  "The app shell could not start",
];
for (const marker of forbiddenEntryMarkers) {
  if (entry.includes(marker)) {
    throw new Error(`dependency-light renderer entry unexpectedly contains ${marker}`);
  }
}
if (!entry.includes("管理器界面无法启动")) {
  throw new Error("dependency-light renderer entry does not contain the static crash fallback");
}
if (!entry.includes("bootstrap-") || !entry.includes("import(")) {
  throw new Error("renderer entry does not dynamically load a separate bootstrap chunk");
}
if (/rel="modulepreload"[^>]+bootstrap-/i.test(html)) {
  throw new Error("bootstrap chunk is eagerly preloaded with the dependency-light entry");
}

const otherJavaScript = readdirSync(assetsPath)
  .filter((name) => name.endsWith(".js") && name !== entryName)
  .map((name) => readFileSync(join(assetsPath, name), "utf8"));
if (!otherJavaScript.some((source) => source.includes("react.transitional.element"))) {
  throw new Error("React was not isolated into a non-entry renderer chunk");
}

console.log(`renderer entry isolation verified: ${entryName}`);
