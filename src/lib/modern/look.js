// Spotiamp+'s own windows (the Library, the lyrics, the dialogs...) in a
// modern skin's look, as Winamp 5 drew its Media Library: in the skin's
// standard frame (wasabi.standardframe.*), with the skin's list colours. The
// engine draws the frame here once per title, off the screens, and it's
// handed over as HTML with the bitmaps and fonts it uses; modern-look.js
// paints it in the other windows. A skin without a frame of its own (Winamp
// lent one from its built-in skin) gets a plain one in its colours.

/** What Winamp's frames say on their title bars, for each kind of window. */
export const LOOK_FRAMES = {
  window: ["Library", "Lyrics", "Visualizer", "Album Art", "Listening Stats", "Skin Museum", "What's New", "Keyboard Shortcuts"],
  dialog: ["Jump to File", "Jump to Time", "Open Location", "Save as a List"],
};

/** The skin's frames for each kind, the first it has. */
const GROUPS = {
  window: ["wasabi.standardframe.nostatusbar", "wasabi.standardframe.statusbar"],
  dialog: ["wasabi.standardframe.modal", "wasabi.standardframe.nostatusbar", "wasabi.standardframe.statusbar"],
};

/** Where the frame is drawn: away from the screens (the window is there only
 *  where the skin's windows are, so none of this is ever seen). */
const STAGE = "position:absolute;left:-30000px;top:-30000px;width:1px;height:1px;";
const W = 420;
const H = 300;

/** @param {string} text */
const xmlText = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const nextFrame = () => new Promise((r) => requestAnimationFrame(() => r(undefined)));

/** Each part that does something gets it written on it, for the other windows. @param {any} obj */
function annotate(obj) {
  const div = obj.getDiv?.();
  if (div) {
    if (obj._action) div.dataset.spAction = String(obj._action).toUpperCase();
    if (obj._resize) div.dataset.spResize = String(obj._resize).toLowerCase();
  }
  for (const child of obj._children ?? []) annotate(child);
}

/**
 * Where the frame keeps its contents: Winamp's frame scripts are given that
 * box (x, y, w, h and which of them count from the far side) as their param.
 * @param {any} container @returns {{ left: number, top: number, right: number, bottom: number } | null}
 */
function clientOf(container) {
  const group = container._layouts?.[0]?._children?.[0];
  const param = (group?._systemObjects ?? [])
    .map((/** @type {any} */ s) => String(s._param ?? ""))
    .find((/** @type {string} */ p) => /^\s*-?\d+\s*(,\s*-?\d+\s*){3}/.test(p));
  if (!param) return null;
  const [x, y, w, h, rx = 0, ry = 0, rw = 0, rh = 0] = param.split(",").map((/** @type {string} */ n) => Number(n) || 0);
  const left = rx ? W + x : x;
  const top = ry ? H + y : y;
  const width = rw ? W + w : w;
  const height = rh ? H + h : h;
  const box = { left, top, right: W - left - width, bottom: H - top - height };
  const sane = Object.values(box).every((v) => v >= 0 && v < 120);
  return sane ? box : null;
}

/** @param {HTMLElement} layout how many of its parts show a picture */
function drawnParts(layout) {
  let drawn = 0;
  for (const el of layout.querySelectorAll("*")) {
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    const style = getComputedStyle(el);
    if (style.display !== "none" && style.opacity !== "0" && style.backgroundImage.includes("url(")) drawn++;
  }
  return drawn;
}

/**
 * One frame, titled, drawn by the engine and taken down as HTML. Its window
 * isn't one of the skin's: it's out of the engine's list (the region, the
 * saved spots, the menus) from the start.
 * @param {any} uiRoot @param {any} engine @param {string} group @param {string} title @param {HTMLElement} stage
 */
async function drawFrame(uiRoot, engine, group, title, stage) {
  const id = `spotiamp.look.${Math.random().toString(36).slice(2)}`;
  const xml =
    `<container id="${id}" name="${xmlText(title)}" default_visible="1">` +
    `<layout id="normal" w="${W}" h="${H}"><group id="${group}" x="0" y="0" w="0" h="0" relatw="1" relath="1"/></layout>` +
    `</container>`;
  const node = engine.parseXmlFragment(xml).children[0].children.find((/** @type {any} */ n) => n.name === "container");
  const container = await engine.container(node);
  const list = uiRoot._containers;
  if (list.includes(container)) list.splice(list.indexOf(container), 1);
  container.draw();
  stage.appendChild(container.getDiv());
  container.init();
  container.setLocation(0, 0);
  await nextFrame();
  await nextFrame();
  const layout = container._activeLayout;
  if (!layout) return null;
  annotate(layout);
  const div = layout.getDiv();
  return { container, html: div.outerHTML, drawn: drawnParts(div), client: clientOf(container) };
}

// ---- colours ----

/** @typedef {[number, number, number]} Rgb */

/** @param {any} uiRoot @param {string} id @param {number} [depth] @returns {Rgb | null} */
function skinColor(uiRoot, id, depth = 0) {
  const lower = id.toLowerCase();
  const color = [...uiRoot._colors].reverse().find((/** @type {any} */ c) => String(c._id).toLowerCase() === lower);
  if (!color) return null;
  const value = String(color.getValue() ?? "").trim();
  if (/^\d+\s*,\s*\d+\s*,\s*\d+/.test(value)) {
    const rgb = uiRoot._getGammaGroup(color.getGammaGroup()).transformColor2rgb(value);
    return /** @type {Rgb} */ (rgb.map((/** @type {number} */ v) => Math.round(Math.min(255, Math.max(0, v)))));
  }
  // a colour named after another one
  return value && depth < 4 ? skinColor(uiRoot, value, depth + 1) : null;
}

/** @param {any} uiRoot @param {string[]} ids */
const firstColor = (uiRoot, ids) => ids.map((id) => skinColor(uiRoot, id)).find(Boolean) ?? null;

/** @param {Rgb} a @param {Rgb} b @param {number} t */
const mix = (a, b, t) => /** @type {Rgb} */ (a.map((v, i) => Math.round(v + (b[i] - v) * t)));
/** @param {Rgb} c */
const lum = (c) => 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
/** @param {Rgb} c */
const css = (c) => `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
/** @type {Rgb} */
const BLACK = [0, 0, 0];
/** @type {Rgb} */
const WHITE = [255, 255, 255];

/**
 * The frame's own colour, from its pictures: for a skin that names no list
 * colours (Winamp lent its built-in skin's), the lists are made from it.
 * @param {string[]} urls @returns {Promise<Rgb | null>}
 */
async function averageColor(urls) {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 24;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  let r = 0, g = 0, b = 0, weight = 0;
  for (const url of urls.slice(0, 40)) {
    const img = new Image();
    img.src = url;
    try {
      await img.decode();
    } catch {
      continue;
    }
    ctx.clearRect(0, 0, 24, 24);
    ctx.drawImage(img, 0, 0, 24, 24);
    const area = Math.min(img.width * img.height, 40000);
    const px = ctx.getImageData(0, 0, 24, 24).data;
    for (let i = 0; i < px.length; i += 4) {
      // Winamp's transparent magenta isn't part of the picture
      if (px[i] > 250 && px[i + 1] < 5 && px[i + 2] > 250) continue;
      const w = (px[i + 3] / 255) * area;
      r += px[i] * w;
      g += px[i + 1] * w;
      b += px[i + 2] * w;
      weight += w;
    }
  }
  return weight ? [Math.round(r / weight), Math.round(g / weight), Math.round(b / weight)] : null;
}

/**
 * The colours Spotiamp+'s windows are drawn in: the skin's own where it
 * names them (the ones Winamp's Media Library used), made from the frame's
 * colour where it doesn't.
 * @param {any} uiRoot @param {Rgb | null} frameColor
 */
function paletteOf(uiRoot, frameColor) {
  const base = frameColor ?? [64, 72, 92];
  const pick = (/** @type {string[]} */ ids) => firstColor(uiRoot, ids);
  /** text that can be read on `bg` (a skin's own, unless it can't) @param {Rgb | null} text @param {Rgb} bg @param {Rgb} tint */
  const readable = (text, bg, tint) =>
    text && Math.abs(lum(text) - lum(bg)) >= 70 ? text : mix(tint, lum(bg) < 128 ? WHITE : BLACK, 0.85);
  const windowBg = pick(["wasabi.window.background"]) ?? base;
  const listBg = pick(["wasabi.list.background"]) ?? mix(base, BLACK, 0.78);
  const dark = lum(listBg) < 128;
  const listText = readable(pick(["wasabi.list.text", "studio.list.text"]), listBg, base);
  const current = readable(pick(["wasabi.list.text.current"]) ?? mix(listText, dark ? WHITE : BLACK, 0.6), listBg, listText);
  // the selected row keeps the list's text: a highlight that hides it is toned down
  let selBg =
    pick(["wasabi.list.text.selected.background", "wasabi.list.item.selected", "studio.list.item.selected"]) ??
    mix(listBg, listText, 0.3);
  if (Math.abs(lum(selBg) - lum(listText)) < 70) selBg = mix(listBg, listText, 0.3);
  const windowText = readable(pick(["wasabi.window.text", "wasabi.text.color"]), windowBg, windowBg);
  const buttonText = readable(pick(["wasabi.button.text", "studio.button.text"]), windowBg, windowText);
  const headerBg = pick(["wasabi.list.column.background"]) ?? mix(windowBg, BLACK, 0.25);
  const headerText = readable(pick(["wasabi.list.column.text"]), headerBg, windowText);
  const border = pick(["wasabi.border.sunken", "wasabi.list.column.separator", "studio.list.column.separator"]) ?? mix(windowBg, BLACK, 0.45);
  return {
    windowBg: css(windowBg),
    windowText: css(windowText),
    listBg: css(listBg),
    listText: css(listText),
    current: css(current),
    selBg: css(selBg),
    buttonText: css(buttonText),
    headerBg: css(headerBg),
    headerText: css(headerText),
    border: css(border),
    accent: css(base),
  };
}

/**
 * The skin's look for Spotiamp+'s windows (see modern-look.js for its use).
 * @param {any} uiRoot @param {any} engine @param {string} skin
 */
export async function buildLook(uiRoot, engine, skin) {
  const root = uiRoot.getRootDiv();
  const stage = document.createElement("div");
  stage.setAttribute("style", STAGE);
  root.appendChild(stage);
  /** @type {Record<string, string>} */
  const frames = {};
  /** @type {Record<string, { left: number, top: number, right: number, bottom: number }>} */
  const client = {};
  try {
    for (const [kind, titles] of Object.entries(LOOK_FRAMES)) {
      const group = GROUPS[/** @type {keyof typeof GROUPS} */ (kind)].find((id) => uiRoot.getGroupDef(id));
      if (!group) continue;
      for (const title of titles) {
        const frame = await drawFrame(uiRoot, engine, group, title, stage);
        // a frame of pictures Winamp had and the skin doesn't: none at all
        if (!frame || frame.drawn < 3) break;
        frames[`${kind}:${title}`] = frame.html;
        client[kind] ??= frame.client ?? { left: 6, top: 20, right: 6, bottom: 8 };
      }
    }
  } finally {
    stage.remove();
  }

  // the pictures and fonts those frames use (the engine has them on its root)
  const html = Object.values(frames).join("");
  const names = [...new Set(html.match(/var\(--[\w-]+/g)?.map((v) => v.slice(4)) ?? [])];
  const style = getComputedStyle(root);
  const vars = [];
  const urls = [];
  for (const name of names) {
    const value = style.getPropertyValue(name).trim();
    if (!value) continue;
    vars.push(`${name}: ${value};`);
    const url = value.match(/^url\(["']?(data:[^"')]+)["']?\)$/)?.[1];
    if (url && name.startsWith("--bitmap")) urls.push(url);
  }
  const fontCss = document.getElementById("truetypefont-css")?.textContent ?? "";
  const fonts = (fontCss.match(/@font-face\s*{[^}]*}/g) ?? []).filter((rule) => {
    const family = rule.match(/font-family:\s*'([^']+)'/)?.[1];
    return family && html.includes(family);
  });

  return {
    skin,
    frames,
    client,
    vars: vars.join("\n"),
    fonts: fonts.join("\n"),
    colors: paletteOf(uiRoot, await averageColor(urls)),
  };
}
