import { describe, expect, it, vi } from "vitest";

import { managerApi } from "../services/managerApi";
import { installGlobalErrorHandlers } from "./globalErrors";

vi.mock("../services/managerApi", () => ({
  managerApi: {
    reportFrontendError: vi.fn(() => Promise.resolve()),
  },
}));

const reportFrontendError = vi.mocked(managerApi.reportFrontendError);

describe("installGlobalErrorHandlers", () => {
  it("logs sync errors as fatal, logs promise rejections as non-fatal, and installs once", () => {
    const fatal = vi.fn();
    window.addEventListener("cam:fatal", fatal);

    installGlobalErrorHandlers();
    installGlobalErrorHandlers();

    const syncError = new Error("render broke");
    window.dispatchEvent(new ErrorEvent("error", { message: syncError.message, error: syncError }));

    expect(reportFrontendError).toHaveBeenCalledTimes(1);
    expect(reportFrontendError).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "window.error", message: "render broke" }),
    );
    expect(fatal).toHaveBeenCalledTimes(1);

    const rejection = new Event("unhandledrejection") as PromiseRejectionEvent;
    Object.defineProperty(rejection, "reason", { value: new Error("background failed") });
    window.dispatchEvent(rejection);

    expect(reportFrontendError).toHaveBeenCalledTimes(2);
    expect(reportFrontendError).toHaveBeenLastCalledWith(
      expect.objectContaining({ kind: "unhandledrejection", message: "background failed" }),
    );
    expect(fatal).toHaveBeenCalledTimes(1);

    window.removeEventListener("cam:fatal", fatal);
  });

  it("ignores benign ResizeObserver noise entirely", () => {
    const fatal = vi.fn();
    window.addEventListener("cam:fatal", fatal);
    installGlobalErrorHandlers();

    window.dispatchEvent(
      new ErrorEvent("error", {
        message: "ResizeObserver loop completed with undelivered notifications.",
      }),
    );

    expect(reportFrontendError).not.toHaveBeenCalled();
    expect(fatal).not.toHaveBeenCalled();
    window.removeEventListener("cam:fatal", fatal);
  });

  it("still escalates non-Error throwables to fatal", () => {
    const fatal = vi.fn();
    window.addEventListener("cam:fatal", fatal);
    installGlobalErrorHandlers();

    // `throw "boom"` — event.error is set but is not an Error instance.
    window.dispatchEvent(new ErrorEvent("error", { message: "boom", error: "boom" }));

    expect(reportFrontendError).toHaveBeenCalledTimes(1);
    expect(fatal).toHaveBeenCalledTimes(1);
    window.removeEventListener("cam:fatal", fatal);
  });

  it("reports error-less window.error events without escalating to fatal", () => {
    const fatal = vi.fn();
    window.addEventListener("cam:fatal", fatal);
    installGlobalErrorHandlers();

    // e.g. a failed resource load: the event carries a message but no Error.
    window.dispatchEvent(new ErrorEvent("error", { message: "Script load failed" }));

    expect(reportFrontendError).toHaveBeenCalledTimes(1);
    expect(reportFrontendError).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "window.error", message: "Script load failed" }),
    );
    expect(fatal).not.toHaveBeenCalled();
    window.removeEventListener("cam:fatal", fatal);
  });
});
