import { currentPlatform, type Platform } from "./platform";

/**
 * Production WebView browser-chrome policy.
 *
 * Release builds must not expose browser chrome (Print / Reload / Inspect).
 * Editable controls keep the platform text-editing affordances so copy/cut/paste
 * still work via keyboard and, where the engine shows one, a native edit menu.
 *
 * Dev builds leave the default menu and shortcuts alone so Reload / DevTools
 * stay available. The explicit crash-screen reload button is not a keyboard
 * event, so it remains available in release builds.
 */

const EDITABLE_SELECTOR = [
  "input",
  "textarea",
  '[contenteditable=""]',
  '[contenteditable="true"]',
  '[contenteditable="plaintext-only"]',
  '[role="textbox"]',
].join(", ");

function eventTargetElement(target: EventTarget | null): Element | null {
  if (target instanceof Text) return target.parentElement;
  if (target instanceof Element) return target;
  return null;
}

/** True when the event target (or an ancestor) is a text-editing control. */
export function isEditableContextTarget(target: EventTarget | null): boolean {
  const start = eventTargetElement(target);
  if (!start) return false;
  const el = start.closest(EDITABLE_SELECTOR);
  if (!el) {
    // Inherited contentEditable is not always reflected as an attribute match.
    let node: Element | null = start;
    while (node) {
      if (node instanceof HTMLElement && node.isContentEditable) return true;
      node = node.parentElement;
    }
    return false;
  }
  if (el instanceof HTMLInputElement) {
    // Buttons/checkboxes are "input" but not text-editing surfaces.
    const type = (el.type || "text").toLowerCase();
    return ![
      "button",
      "checkbox",
      "radio",
      "submit",
      "reset",
      "file",
      "image",
      "range",
      "color",
      "hidden",
    ].includes(type);
  }
  if (el instanceof HTMLTextAreaElement) return true;
  if (el.getAttribute("role") === "textbox") return true;
  // Prefer the live contentEditable flag; also honor the attribute so jsdom
  // (and similar test envs where isContentEditable stays false) still match.
  if (el instanceof HTMLElement && el.isContentEditable) return true;
  const ce = el.getAttribute("contenteditable");
  return ce === "" || ce === "true" || ce === "plaintext-only";
}

/**
 * Browser-level accelerators that are not product commands. Keep navigation
 * chords available inside editable controls when they are also text-editing
 * shortcuts (for example Option+Arrow on macOS).
 */
export function shouldBlockWebviewShortcut(
  event: KeyboardEvent,
  platform: Platform = currentPlatform(),
): boolean {
  const key = event.key.toLowerCase();
  const code = event.code.toLowerCase();
  const mac = platform === "macos";
  const primary = mac ? event.metaKey : event.ctrlKey;

  const matchesPhysicalKey = (letter: string) =>
    key === letter || code === `key${letter}`;

  if (["f3", "f5", "f12"].includes(key) || ["f3", "f5", "f12"].includes(code)) {
    return true;
  }
  if (
    key === "browserback" ||
    key === "browserforward" ||
    code === "browserback" ||
    code === "browserforward"
  ) {
    return true;
  }

  if (primary && ["f", "p", "r", "s", "u"].some(matchesPhysicalKey)) return true;
  if (
    (!mac && event.ctrlKey && event.shiftKey && ["i", "j", "c"].some(matchesPhysicalKey)) ||
    (mac && event.metaKey && event.altKey && ["i", "j", "c"].some(matchesPhysicalKey))
  ) {
    return true;
  }

  const editable = isEditableContextTarget(event.target);
  // Cmd+[ / ] is WebKit history even in a text field. Alt+Arrow is history on
  // Windows, but Option+Arrow is standard word navigation in macOS editors.
  if (
    event.metaKey &&
    (key === "[" || key === "]" || code === "bracketleft" || code === "bracketright")
  ) {
    return true;
  }
  if (event.altKey && (key === "arrowleft" || key === "arrowright")) {
    if (mac && editable) return false;
    return true;
  }
  if (editable) return false;
  return false;
}

/**
 * Install the production policy. No-op when `enabled` is false (dev builds).
 * Returns a disposer for tests.
 */
export function installContextMenuPolicy(enabled = !import.meta.env.DEV): () => void {
  if (!enabled || typeof document === "undefined") {
    return () => {};
  }

  const onContextMenu = (event: Event) => {
    if (isEditableContextTarget(event.target)) return;
    event.preventDefault();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (!shouldBlockWebviewShortcut(event)) return;
    event.preventDefault();
    event.stopPropagation();
  };

  const onMouseNavigation = (event: MouseEvent) => {
    // Mouse buttons 3/4 map to browser back/forward on WebView2 and WebKit.
    if (event.button !== 3 && event.button !== 4) return;
    event.preventDefault();
    event.stopPropagation();
  };

  document.addEventListener("contextmenu", onContextMenu, true);
  document.addEventListener("keydown", onKeyDown, true);
  document.addEventListener("mousedown", onMouseNavigation, true);
  document.addEventListener("mouseup", onMouseNavigation, true);
  document.addEventListener("auxclick", onMouseNavigation, true);
  return () => {
    document.removeEventListener("contextmenu", onContextMenu, true);
    document.removeEventListener("keydown", onKeyDown, true);
    document.removeEventListener("mousedown", onMouseNavigation, true);
    document.removeEventListener("mouseup", onMouseNavigation, true);
    document.removeEventListener("auxclick", onMouseNavigation, true);
  };
}
