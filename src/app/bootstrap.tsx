import React from "react";
import ReactDOM from "react-dom/client";

import { RootCrashBoundary } from "./RootCrashBoundary";
import "./styles.css";

/** Start the renderer while keeping App and its providers behind a catchable import. */
export async function bootstrap(): Promise<void> {
  const root = document.getElementById("root");
  if (!(root instanceof HTMLElement)) {
    throw new Error("Missing #root mount element");
  }

  const [platform, theme, contextMenu, globalErrors] = await Promise.all([
    import("./platform"),
    import("./theme"),
    import("./contextMenuPolicy"),
    import("./globalErrors"),
  ]);

  document.documentElement.dataset.platform = platform.currentPlatform();
  document.documentElement.dataset.theme = theme.resolveInitialTheme();
  globalErrors.installGlobalErrorHandlers();
  contextMenu.installContextMenuPolicy();

  const { App } = await import("./App");
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <RootCrashBoundary>
        <App />
      </RootCrashBoundary>
    </React.StrictMode>,
  );
}
