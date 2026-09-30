import { emitWindowEvent } from "./events.svelte.js";

// The main window's keys, from any window: pressed here, they're handed to the
// main window, which does exactly what it does for its own keys (Z X C V B,
// Space, S, R, J, Q, F, L, O / Shift+O, the arrows, Ctrl+D). One set of shortcuts
// for the whole app instead of each window knowing a different few.
const MAIN_WINDOW_KEYS = new Set([
  "z", "x", "c", "v", "b", " ", "s", "r", "j", "q", "l", "o", "f",
  "arrowup", "arrowdown", "arrowleft", "arrowright",
]);

/**
 * Pass the main window's shortcuts on from this window. Keys typed into a text
 * field stay there. Returns the cleanup, so it can go straight into onMount.
 * @param {{ keys?: Set<string> }} [options] which plain keys to pass on; a
 *   window that uses letters itself (the Library searches as you type) passes
 *   an empty set and keeps just Ctrl+D.
 */
export function forwardShortcuts({ keys = MAIN_WINDOW_KEYS } = {}) {
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    if (e.altKey || e.metaKey || e.defaultPrevented) return;
    const k = e.key.toLowerCase();
    // Ctrl+D (scale), Ctrl+V (stop after current) and F1 (the key guide) work
    // from every window, even the Library, whose plain letters search instead
    const always = (e.ctrlKey && (k === "d" || k === "v")) || (!e.ctrlKey && k === "f1");
    if (!always && (e.ctrlKey || !keys.has(k))) return;
    e.preventDefault();
    emitWindowEvent("forwardedKey", { key: e.key, shift: e.shiftKey, ctrl: e.ctrlKey });
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
