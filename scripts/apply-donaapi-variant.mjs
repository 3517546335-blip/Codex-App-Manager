#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";

const edits = [
  {
    path: "src/app/components.tsx",
    replacements: [
      ["S1_API_URL", "DONA_API_URL"],
      ["https://s1api.com", "https://donaapi.com"],
    ],
  },
  {
    path: "src/app/views/CodexConfig.tsx",
    replacements: [
      ["s1api", "donaapi"],
      ["S1API", "DonaAPI"],
    ],
  },
  {
    path: "src/app/views/Home.tsx",
    replacements: [["S1API", "DonaAPI"]],
  },
  {
    path: "src/app/SpeedApiBanner.test.tsx",
    replacements: [
      ["S1API", "DonaAPI"],
      ["https://s1api.com", "https://donaapi.com"],
    ],
  },
  {
    path: "src/services/managerApi.ts",
    replacements: [
      ["s1api", "donaapi"],
      ["S1API", "DonaAPI"],
    ],
  },
  {
    path: "src-tauri/src/commands.rs",
    replacements: [
      ["s1api", "donaapi"],
      ["S1API", "DonaAPI"],
    ],
  },
  {
    path: "README.md",
    replacements: [
      ["q4549286/Codex-Desktop-Releases", "q4549286/Codex-Desktop-DonaAPI-Releases"],
      ["S1API", "DonaAPI"],
    ],
  },
];

for (const edit of edits) {
  let content = readFileSync(edit.path, "utf8");
  for (const [from, to] of edit.replacements) {
    if (content.includes(from)) {
      content = content.replaceAll(from, to);
    } else if (!content.includes(to)) {
      throw new Error(`${edit.path}: expected variant marker not found: ${from}`);
    }
  }
  writeFileSync(edit.path, content);
}

const variantSources = edits.map((edit) => readFileSync(edit.path, "utf8")).join("\n");
for (const forbidden of ["https://s1api.com", "S1_API_URL", "S1API"]) {
  if (variantSources.includes(forbidden)) {
    throw new Error(`DonaAPI variant still contains: ${forbidden}`);
  }
}

console.log("DonaAPI variant applied: https://donaapi.com");
