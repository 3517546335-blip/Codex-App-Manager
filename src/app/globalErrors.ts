import { managerApi } from "../services/managerApi";

let installed = false;

// Browser-noise messages that are neither reportable nor fatal: the
// ResizeObserver pair is a well-known benign loop warning some engines
// surface as a window.error.
const BENIGN_MESSAGES = [
  "ResizeObserver loop limit exceeded",
  "ResizeObserver loop completed with undelivered notifications",
];

export function installGlobalErrorHandlers() {
  if (installed) {
    return;
  }
  installed = true;

  window.addEventListener("error", (event) => {
    const message =
      event.error instanceof Error ? event.error.message : String(event.message ?? "");
    if (BENIGN_MESSAGES.some((benign) => message.includes(benign))) {
      return;
    }
    const error =
      event.error instanceof Error ? event.error : new Error(message || "Unknown window.error");
    void managerApi.reportFrontendError({
      kind: "window.error",
      message: error.message,
      stack: error.stack ?? null,
      componentStack: null,
    });
    // Escalate to the full-screen fatal state whenever the window saw a real
    // thrown value — including non-Error throwables like `throw "boom"`. Only
    // error-LESS events (failed resource loads and other engine-synthesized
    // noise, where event.error is null/undefined) are logged without blanking
    // a perfectly healthy UI.
    if (event.error != null) {
      window.dispatchEvent(new CustomEvent("cam:fatal", { detail: { error } }));
    }
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const error = reason instanceof Error ? reason : new Error(String(reason));
    void managerApi.reportFrontendError({
      kind: "unhandledrejection",
      message: error.message,
      stack: error.stack ?? null,
      componentStack: null,
    });
  });
}
