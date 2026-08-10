import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { SpeedApiBanner } from "./components";

describe("SpeedApiBanner", () => {
  it("keeps the DonaAPI promotion visible", () => {
    render(<SpeedApiBanner />);
    const banner = screen.getByRole("button", { name: /让每一次 AI 调用更简单/ });
    expect(banner).toHaveAttribute("title", "https://donaapi.com");
    expect(screen.getByText("https://donaapi.com")).toBeInTheDocument();
  });
});
