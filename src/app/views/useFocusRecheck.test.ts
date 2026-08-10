import { describe, expect, it } from "vitest";

import { normalizePath } from "../paths";
import { installIdentity } from "./useFocusRecheck";

describe("installIdentity", () => {
  it("is null for an absent install", () => {
    expect(installIdentity(null)).toBeNull();
    expect(installIdentity(undefined)).toBeNull();
  });

  it("keys mac installs on build + raw (case-sensitive) path", () => {
    const a = installIdentity({ build: 100, path: "/Applications/Codex.app" });
    const b = installIdentity({ build: 100, path: "/Applications/codex.app" });
    // Mac paths are case-sensitive — these are DIFFERENT installs.
    expect(a).not.toBe(b);
    expect(installIdentity({ build: 101, path: "/Applications/Codex.app" })).not.toBe(a);
  });

  it("folds Windows path casing / separators so a cosmetic diff isn't drift", () => {
    const a = installIdentity(
      { version: "1.0.0", path: "C:\\Program Files\\Codex" },
      normalizePath,
    );
    const b = installIdentity(
      { version: "1.0.0", path: "c:/program files/codex/" },
      normalizePath,
    );
    // Same install, different spelling — must be the SAME identity.
    expect(a).toBe(b);
    // A real version change is still drift.
    expect(installIdentity({ version: "2.0.0", path: "C:\\Program Files\\Codex" }, normalizePath)).not.toBe(a);
  });
});
