import { flushSync } from "react-dom";

type ViewTransitionDocument = Document & {
  startViewTransition?: (callback: () => void) => unknown;
};

/** Cross-fade the whole UI between old and new state (theme or language).
 *  Uses the View Transitions API where available (Tauri's WKWebView on macoS,
 *  WebView2/Chromium on Windows); otherwise applies instantly. Honors
 *  prefers-reduced-motion. `flushSync` forces the DOM mutation to happen inside
 *  the transition's capture window, so the before/after snapshots differ. */
export function withViewTransition(apply: () => void): void {
  const doc = document as ViewTransitionDocument;
  const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  if (typeof doc.startViewTransition === "function" && !reduce) {
    doc.startViewTransition(() => {
      flushSync(apply);
    });
  } else {
    apply();
  }
}
