import { afterEach, describe, expect, it } from "vitest";

import {
  installContextMenuPolicy,
  isEditableContextTarget,
  shouldBlockWebviewShortcut,
} from "./contextMenuPolicy";

describe("isEditableContextTarget", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("accepts text inputs and textareas", () => {
    document.body.innerHTML = `
      <input id="t" type="text" />
      <input id="n" type="number" />
      <textarea id="a"></textarea>
      <div id="ce" contenteditable="true">hello</div>
      <div id="tb" role="textbox">x</div>
    `;
    expect(isEditableContextTarget(document.getElementById("t"))).toBe(true);
    expect(isEditableContextTarget(document.getElementById("n"))).toBe(true);
    expect(isEditableContextTarget(document.getElementById("a"))).toBe(true);
    expect(isEditableContextTarget(document.getElementById("ce"))).toBe(true);
    expect(isEditableContextTarget(document.getElementById("tb"))).toBe(true);
  });

  it("accepts Text nodes inside contenteditable", () => {
    document.body.innerHTML = `<div id="ce" contenteditable="true">hello</div>`;
    const text = document.getElementById("ce")!.firstChild;
    expect(text).toBeInstanceOf(Text);
    expect(isEditableContextTarget(text)).toBe(true);
  });

  it("rejects non-text inputs, select, and ordinary chrome", () => {
    document.body.innerHTML = `
      <input id="cb" type="checkbox" />
      <button id="b">go</button>
      <select id="s"><option>a</option></select>
      <div id="d">plain</div>
    `;
    expect(isEditableContextTarget(document.getElementById("cb"))).toBe(false);
    expect(isEditableContextTarget(document.getElementById("b"))).toBe(false);
    expect(isEditableContextTarget(document.getElementById("s"))).toBe(false);
    expect(isEditableContextTarget(document.getElementById("d"))).toBe(false);
    expect(isEditableContextTarget(null)).toBe(false);
  });

  it("walks up to an editable ancestor", () => {
    document.body.innerHTML = `<div contenteditable="true"><span id="s">x</span></div>`;
    expect(isEditableContextTarget(document.getElementById("s"))).toBe(true);
  });
});

describe("shouldBlockWebviewShortcut", () => {
  it.each([
    ["F5", {}, "windows"],
    ["F3", {}, "windows"],
    ["F12", {}, "macos"],
    ["f", { ctrlKey: true }, "windows"],
    ["r", { ctrlKey: true }, "windows"],
    ["R", { metaKey: true, shiftKey: true }, "macos"],
    ["p", { metaKey: true }, "macos"],
    ["s", { ctrlKey: true }, "windows"],
    ["u", { ctrlKey: true }, "windows"],
    ["i", { ctrlKey: true, shiftKey: true }, "windows"],
    ["j", { metaKey: true, altKey: true }, "macos"],
    ["BrowserBack", {}, "windows"],
    ["BrowserForward", {}, "macos"],
  ] as const)("blocks browser command %s", (key, init, platform) => {
    expect(
      shouldBlockWebviewShortcut(new KeyboardEvent("keydown", { key, ...init }), platform),
    ).toBe(true);
  });

  it("uses the physical key code for browser accelerators on non-Latin layouts", () => {
    expect(
      shouldBlockWebviewShortcut(
        new KeyboardEvent("keydown", { key: "к", code: "KeyR", ctrlKey: true }),
        "windows",
      ),
    ).toBe(true);
    expect(
      shouldBlockWebviewShortcut(
        new KeyboardEvent("keydown", { key: "ش", code: "KeyP", ctrlKey: true }),
        "windows",
      ),
    ).toBe(true);
    expect(
      shouldBlockWebviewShortcut(
        new KeyboardEvent("keydown", { key: "х", code: "BracketLeft", metaKey: true }),
        "macos",
      ),
    ).toBe(true);
  });

  it("uses Command for browser commands on macOS and preserves Control editing chords", () => {
    document.body.innerHTML = `<input id="edit" />`;
    const edit = document.getElementById("edit")!;
    const controlU = new KeyboardEvent("keydown", { key: "u", ctrlKey: true });
    const commandU = new KeyboardEvent("keydown", { key: "u", metaKey: true });
    Object.defineProperty(controlU, "target", { value: edit });
    Object.defineProperty(commandU, "target", { value: edit });

    expect(shouldBlockWebviewShortcut(controlU, "macos")).toBe(false);
    expect(shouldBlockWebviewShortcut(commandU, "macos")).toBe(true);
  });

  it("blocks history chords on chrome but preserves text navigation", () => {
    document.body.innerHTML = `<div id="plain"></div><input id="edit" />`;
    const plain = document.getElementById("plain")!;
    const edit = document.getElementById("edit")!;
    const plainEvent = new KeyboardEvent("keydown", { key: "ArrowLeft", altKey: true });
    Object.defineProperty(plainEvent, "target", { value: plain });
    const editEvent = new KeyboardEvent("keydown", { key: "ArrowLeft", altKey: true });
    Object.defineProperty(editEvent, "target", { value: edit });

    expect(shouldBlockWebviewShortcut(plainEvent, "macos")).toBe(true);
    expect(shouldBlockWebviewShortcut(editEvent, "macos")).toBe(false);
  });

  it("blocks platform history chords even while a text input is focused", () => {
    document.body.innerHTML = `<input id="edit" />`;
    const edit = document.getElementById("edit")!;
    const windowsBack = new KeyboardEvent("keydown", { key: "ArrowLeft", altKey: true });
    const macBack = new KeyboardEvent("keydown", { key: "[", metaKey: true });
    Object.defineProperty(windowsBack, "target", { value: edit });
    Object.defineProperty(macBack, "target", { value: edit });

    expect(shouldBlockWebviewShortcut(windowsBack, "windows")).toBe(true);
    expect(shouldBlockWebviewShortcut(macBack, "macos")).toBe(true);
  });
});

describe("installContextMenuPolicy", () => {
  let dispose: (() => void) | null = null;

  afterEach(() => {
    dispose?.();
    dispose = null;
    document.body.innerHTML = "";
  });

  it("prevents default on non-editable targets when enabled", () => {
    document.body.innerHTML = `<div id="d">x</div><input id="t" type="text" />`;
    dispose = installContextMenuPolicy(true);

    const plain = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.getElementById("d")!.dispatchEvent(plain);
    expect(plain.defaultPrevented).toBe(true);

    const edit = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.getElementById("t")!.dispatchEvent(edit);
    expect(edit.defaultPrevented).toBe(false);
  });

  it("does not prevent default for Text inside contenteditable", () => {
    document.body.innerHTML = `<div id="ce" contenteditable="true">hello</div>`;
    dispose = installContextMenuPolicy(true);
    const text = document.getElementById("ce")!.firstChild!;
    const edit = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    text.dispatchEvent(edit);
    expect(edit.defaultPrevented).toBe(false);
  });

  it("is a no-op when disabled (dev builds)", () => {
    document.body.innerHTML = `<div id="d">x</div>`;
    dispose = installContextMenuPolicy(false);
    const plain = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.getElementById("d")!.dispatchEvent(plain);
    expect(plain.defaultPrevented).toBe(false);
  });

  it("dispose removes the listener", () => {
    document.body.innerHTML = `<div id="d">x</div>`;
    dispose = installContextMenuPolicy(true);
    dispose();
    dispose = null;
    const plain = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.getElementById("d")!.dispatchEvent(plain);
    expect(plain.defaultPrevented).toBe(false);

    for (const type of ["mousedown", "mouseup", "auxclick"]) {
      const sideButton = new MouseEvent(type, {
        button: 3,
        bubbles: true,
        cancelable: true,
      });
      document.getElementById("d")!.dispatchEvent(sideButton);
      expect(sideButton.defaultPrevented, type).toBe(false);
    }
  });

  it("blocks release keyboard and mouse navigation while installed", () => {
    document.body.innerHTML = `<div id="d">x</div>`;
    dispose = installContextMenuPolicy(true);

    const reload = new KeyboardEvent("keydown", {
      key: "r",
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    document.getElementById("d")!.dispatchEvent(reload);
    expect(reload.defaultPrevented).toBe(true);

    for (const type of ["mousedown", "mouseup", "auxclick"]) {
      const back = new MouseEvent(type, {
        button: 3,
        bubbles: true,
        cancelable: true,
      });
      document.getElementById("d")!.dispatchEvent(back);
      expect(back.defaultPrevented, type).toBe(true);
    }
  });
});
