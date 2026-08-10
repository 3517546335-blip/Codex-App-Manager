import { describe, expect, it } from "vitest";

import {
  MANAGER_UPDATE_CHECK_INTERVAL_MS,
  managerUpdateCheckDue,
} from "./ManagerUpdateNotice";

describe("managerUpdateCheckDue", () => {
  const now = 1_800_000_000_000;

  it("checks when no valid timestamp exists", () => {
    expect(managerUpdateCheckDue(null, now)).toBe(true);
    expect(managerUpdateCheckDue("invalid", now)).toBe(true);
    expect(managerUpdateCheckDue(String(now + 1), now)).toBe(true);
  });

  it("checks at most once per 24 hours", () => {
    expect(managerUpdateCheckDue(String(now - MANAGER_UPDATE_CHECK_INTERVAL_MS + 1), now)).toBe(
      false,
    );
    expect(managerUpdateCheckDue(String(now - MANAGER_UPDATE_CHECK_INTERVAL_MS), now)).toBe(true);
  });
});
