// A modern skin's look on Spotiamp+'s own windows and dialogs: the skin's
// standard frame (made by modern/look.js in the modern skin's window), drawn
// in a shadow root so neither its styles nor ours leak into the other, and
// the skin's list colours for what's inside.
import ENGINE_CSS from "./modern/css/webamp.css?inline";

/**
 * @typedef {{ left: number, top: number, right: number, bottom: number }} Insets
 * @typedef {{
 *   skin: string,
 *   frames: Record<string, string>,
 *   client: Record<string, Insets>,
 *   vars: string,
 *   fonts: string,
 *   colors: Record<string, string>,
 * }} Look
 */

/** A skin with no frame of its own: the plain one's insets. */
const PLAIN_CLIENT = { left: 5, top: 19, right: 5, bottom: 5 };

const FRAME_CSS = `
:host { position: absolute; inset: 0; display: block; }
.sp-frame { position: absolute; inset: 0; display: block; }
.sp-frame > layout { left: 0 !important; top: 0 !important; width: 100% !important; height: 100% !important; }
container.sp-active [activealpha="0"] { opacity: 0; }
container.sp-active [inactivealpha] { opacity: 1; }
container.sp-active .webamp--img.inactivable { background-image: var(--background-image); }
`;

const PLAIN_CSS = `
:host { position: absolute; inset: 0; display: block; }
.sp-plain {
  position: absolute; inset: 0; box-sizing: border-box;
  background: var(--accent);
  border: 1px solid color-mix(in srgb, var(--accent) 45%, #000);
  box-shadow: inset 1px 1px 0 color-mix(in srgb, var(--accent) 60%, #fff);
}
.sp-title {
  position: absolute; left: 0; right: 18px; top: 0; height: 18px;
  display: flex; align-items: center; justify-content: center;
  font: bold 10px Arial, sans-serif; letter-spacing: 0.5px;
  color: var(--text); white-space: nowrap; overflow: hidden;
}
.sp-close {
  position: absolute; right: 3px; top: 3px; width: 13px; height: 12px; padding: 0;
  border: 1px solid color-mix(in srgb, var(--accent) 40%, #000);
  background: color-mix(in srgb, var(--accent) 80%, #fff); color: var(--text);
  font: bold 10px/9px Arial, sans-serif;
}
.sp-close:active { background: color-mix(in srgb, var(--accent) 70%, #000); }
.sp-grip { position: absolute; right: 0; bottom: 0; width: 12px; height: 12px; }
`;

/** @param {string} text */
const escapeHtml = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** The fonts the frames write in: @font-face only works in the page itself. @param {Look | null} look */
export function useLookFonts(look) {
  let el = document.getElementById("modern-look-fonts");
  if (!look?.fonts) {
    el?.remove();
    return;
  }
  if (!el) {
    el = document.createElement("style");
    el.id = "modern-look-fonts";
    document.head.appendChild(el);
  }
  if (el.textContent !== look.fonts) el.textContent = look.fonts;
}

/**
 * The room the frame titled `title` leaves for the contents.
 * @param {Look} look @param {"window" | "dialog"} kind @param {string} title @returns {Insets}
 */
export function frameInsets(look, kind, title) {
  return look.frames[`${kind}:${title}`] ? (look.client[kind] ?? PLAIN_CLIENT) : PLAIN_CLIENT;
}

/**
 * Draw the frame titled `title` (a window's, or a dialog's) in `host`. Its
 * parts that do something say so (data-sp-action: CLOSE, MINIMIZE...;
 * data-sp-resize: the edge or corner they size from).
 * @param {HTMLElement} host @param {Look} look @param {"window" | "dialog"} kind @param {string} title
 * @returns {Insets} where the frame leaves room for the contents
 */
export function paintFrame(host, look, kind, title) {
  const root = host.shadowRoot ?? host.attachShadow({ mode: "open" });
  const html = look.frames[`${kind}:${title}`];
  if (html) {
    root.innerHTML = `<style>${ENGINE_CSS}${FRAME_CSS}:host { ${look.vars} }</style><container class="sp-frame">${html}</container>`;
    // nothing in a skin's frame runs code here (the engine never writes any)
    for (const el of root.querySelectorAll("*")) {
      for (const attr of [...el.attributes]) if (attr.name.startsWith("on")) el.removeAttribute(attr.name);
    }
    return frameInsets(look, kind, title);
  }
  const c = look.colors;
  root.innerHTML =
    `<style>${PLAIN_CSS}</style>` +
    `<div class="sp-plain" style="--accent: ${c.accent}; --text: ${c.windowText}">` +
    `<div class="sp-title">${escapeHtml(title)}</div>` +
    `<button class="sp-close" data-sp-action="CLOSE" aria-label="Close">×</button>` +
    `<div class="sp-grip" data-sp-resize="bottomright"></div></div>`;
  return PLAIN_CLIENT;
}

/** The frame lit, as Winamp drew the focused window's. @param {HTMLElement} host @param {boolean} active */
export function setFrameActive(host, active) {
  host.shadowRoot?.querySelector(".sp-frame")?.classList.toggle("sp-active", active);
}

/**
 * What a press on the frame does: a part's action (`onAction`), a size
 * corner (`onResize`, pass the press on), or else it moves the window
 * (left to the host's own handler, so nothing is done here).
 * @param {HTMLElement} host
 * @param {{ onAction: (action: string) => void, onResize?: (e: PointerEvent, edge: string) => void }} handlers
 * @returns {() => void} undo
 */
export function wireFrame(host, { onAction, onResize }) {
  const root = /** @type {ShadowRoot} */ (host.shadowRoot);
  /** @param {Event} e @param {string} key */
  const partOf = (e, key) =>
    /** @type {HTMLElement | undefined} */ (
      e.composedPath().find((el) => el instanceof HTMLElement && el.dataset?.[key])
    );
  /** @param {PointerEvent} e */
  const down = (e) => {
    if (e.button !== 0) return;
    const action = partOf(e, "spAction");
    if (action) {
      // a button, not a handle: the window stays put
      e.stopPropagation();
      return;
    }
    const edge = partOf(e, "spResize")?.dataset.spResize ?? "";
    if (onResize && /bottom|right/.test(edge) && !/left|top/.test(edge)) {
      e.stopPropagation();
      onResize(e, edge);
    }
  };
  /** @param {MouseEvent} e */
  const click = (e) => {
    const action = partOf(e, "spAction")?.dataset.spAction;
    if (action) onAction(action);
  };
  root.addEventListener("pointerdown", /** @type {EventListener} */ (down));
  root.addEventListener("click", /** @type {EventListener} */ (click));
  return () => {
    root.removeEventListener("pointerdown", /** @type {EventListener} */ (down));
    root.removeEventListener("click", /** @type {EventListener} */ (click));
  };
}

/** The window's insides in the skin's colours (Spotiamp+'s skin variables). @param {Look["colors"]} c */
export function lookColorsCss(c) {
  const vars = {
    plbg: c.listBg,
    plnormal: c.listText,
    plcurrent: c.current,
    plselbg: c.selBg,
    genexitembg: c.listBg,
    genexitemfg: c.listText,
    genexwndbg: c.windowBg,
    genexwndtext: c.windowText,
    genexbtntext: c.buttonText,
    genexselbg: c.selBg,
    genexhdrbg: c.headerBg,
    genexhdrtext: c.headerText,
    genexdivider: c.border,
    titletext: c.windowText,
    titlebarcolor: c.windowBg,
  };
  const lines = Object.entries(vars).map(([name, value]) => `--skin-${name}: ${value} !important;`);
  // the classic skin's pictures and cursors don't go with it
  for (const name of ["genexbtn", "genexbtnp", "gen", "genletters"]) lines.push(`--skin-${name}: initial !important;`);
  return lines.join("\n");
}
