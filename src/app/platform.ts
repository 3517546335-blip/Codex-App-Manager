// Which desktop platform are we rendering for? The backend commands are
// platform-specific (mac_* vs win_*), so the UI dispatches on this.

export type Platform = "macos" | "windows" | "other";

export function currentPlatform(): Platform {
  const p = (navigator.platform || "").toLowerCase();
  const ua = (navigator.userAgent || "").toLowerCase();
  if (p.startsWith("mac") || ua.includes("mac os") || ua.includes("macintosh")) {
    return "macos";
  }
  if (p.startsWith("win") || ua.includes("windows")) {
    return "windows";
  }
  return "other";
}

export function isWindows(): boolean {
  return currentPlatform() === "windows";
}
