import { waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { startRenderer } from "./rendererEntry";
import { disposeStaticCrashPolicy } from "./staticCrashFallback";

describe("startRenderer", () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="root"></div>';
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    disposeStaticCrashPolicy();
    delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    vi.restoreAllMocks();
  });

  it("renders the static surface when the bootstrap module cannot load or evaluate", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    setInternals(invoke);

    await startRenderer(async () => {
      throw new Error("bootstrap chunk evaluation failed");
    });

    expect(document.querySelector("[data-static-crash=true]")).toHaveTextContent(
      "bootstrap chunk evaluation failed",
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "log_frontend_error",
        expect.objectContaining({
          payload: expect.objectContaining({ message: "bootstrap chunk evaluation failed" }),
        }),
      ),
    );
  });

  it("renders the same surface for a synchronous pre-createRoot bootstrap failure", async () => {
    await startRenderer(async () => ({
      bootstrap: () => {
        throw new Error("pre-createRoot failure");
      },
    }));

    expect(document.querySelector("[data-static-crash=true]")).toHaveTextContent(
      "pre-createRoot failure",
    );
  });

  it("keeps the startup browser guard until bootstrap installs the normal policy", async () => {
    const dispose = vi.fn();
    const bootstrap = vi.fn().mockResolvedValue(undefined);

    await startRenderer(async () => ({ bootstrap }), () => dispose);

    expect(bootstrap).toHaveBeenCalledOnce();
    expect(dispose).toHaveBeenCalledOnce();
  });

  it("retains the strict startup guard when bootstrap fails", async () => {
    const dispose = vi.fn();

    await startRenderer(
      async () => ({
        bootstrap: () => {
          throw new Error("bootstrap failed under guard");
        },
      }),
      () => dispose,
    );

    expect(dispose).not.toHaveBeenCalled();
    expect(document.querySelector("[data-static-crash=true]")).toBeInTheDocument();
  });
});

function setInternals(invoke: ReturnType<typeof vi.fn>): void {
  (
    window as typeof window & {
      __TAURI_INTERNALS__?: { invoke: typeof invoke };
    }
  ).__TAURI_INTERNALS__ = { invoke };
}
