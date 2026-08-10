import type { Platform } from "./platform";

export function codexHomeDisplay(platform: Platform): string {
  return platform === "windows" ? "%USERPROFILE%\\.codex" : "~/.codex";
}

/** Normalize a Windows path for comparison: trim, drop the trailing slash,
 *  unify separators to `\`, and lowercase (install roots come back with mixed
 *  `\`/`/` and casing). NOT for mac paths — those are case-sensitive. */
export function normalizePath(value: string): string {
  return value.trim().replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();
}

/** Case-insensitive path equality (Windows). Shared by the Windows home and
 *  settings. */
export function samePath(a: string, b: string): boolean {
  return normalizePath(a) === normalizePath(b);
}
