// Spotiamp+: Winamp 5's docking for a modern skin's windows, as Wasabi's
// WindowTracker did it (api/wnd/wndtrack.cpp and guiobj.cpp, read for how
// they behave):
// - a dragged window snaps to the edges of the screen it's on (its work
//   area) and to the edges of the other windows, the skin's and Spotiamp+'s
//   own, when it comes within 10 pixels of one and would then touch it;
// - dragging the skin's main window (or any with Alt held) takes every
//   window docked to it along, and the ones docked to those (with Alt, all
//   of them), and if one of those docks to something on the way the whole
//   group does; with Shift held nothing docks and nothing comes along;
// - when a window's size changes (a layout switch, a drawer the script
//   opens by resizing it), the windows docked to its bottom and right
//   follow the new edges.
// Rects are page pixels: { left, top, right, bottom }.

const DOCK_DIST = 10;
// rects come from transforms (a screen at 125 %): edges a pixel apart touch
const SLACK = 1;

/** @typedef {{ left: number, top: number, right: number, bottom: number }} Rect */
/** a skin window (and its container's spot then) or one of Spotiamp+'s
 *  @typedef {{ kind: "skin", c: any, rect: Rect, x0: number, y0: number } | { kind: "app", label: string, rect: Rect }} Win */

/** @param {Rect} r @param {number} dx @param {number} dy @returns {Rect} */
const offset = (r, dx, dy) => ({ left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy });
/** @param {number} a @param {number} b */
const same = (a, b) => Math.abs(a - b) <= SLACK;

/** Wasabi's touches(): an edge on an edge, side by side or in line. @param {Rect} a @param {Rect} b */
function touches(a, b) {
  if (same(a.left, b.right) || same(a.right, b.left) || same(a.right, b.right) || same(a.left, b.left)) {
    if (a.bottom >= b.top - SLACK && a.top <= b.bottom + SLACK) return true;
  }
  if (same(a.top, b.bottom) || same(a.bottom, b.top) || same(a.bottom, b.bottom) || same(a.top, b.top)) {
    if (a.right >= b.left - SLACK && a.left <= b.right + SLACK) return true;
  }
  return false;
}

/** Do they cover each other (more than a shared edge)? @param {Rect} a @param {Rect} b */
const intersects = (a, b) =>
  a.left < b.right - SLACK && b.left < a.right - SLACK && a.top < b.bottom - SLACK && b.top < a.bottom - SLACK;

/**
 * Wasabi's autoDock: `z` moved onto the screen's edges and the windows'
 * within the docking distance, keeping its size; a snap that wouldn't leave
 * it touching what it snapped to is undone. Returns the rect and whether it
 * snapped. @param {Rect} z @param {Rect | null} view @param {Rect[]} others @param {boolean} noIntersect
 */
function autoDock(z, view, others, noIntersect) {
  const w = z.right - z.left;
  const h = z.bottom - z.top;
  const near = (/** @type {number} */ a, /** @type {number} */ b) => a > b - DOCK_DIST && a < b + DOCK_DIST;
  let snapped = false;
  const targets = [...(view ? [{ r: view, view: true }] : []), ...others.map((r) => ({ r, view: false })).reverse()];
  for (const { r, view: isView } of targets) {
    const oz = { ...z };
    let doneX = false;
    let doneY = false;
    if (near(z.left, r.left)) {
      z = { ...z, left: r.left, right: r.left + w };
      doneX = true;
    }
    if (!isView && !doneX && near(z.right, r.left)) {
      z = { ...z, right: r.left, left: r.left - w };
      doneX = true;
    }
    if (near(z.top, r.top)) {
      z = { ...z, top: r.top, bottom: r.top + h };
      doneY = true;
    }
    if (!isView && !doneY && near(z.bottom, r.top)) {
      z = { ...z, bottom: r.top, top: r.top - h };
      doneY = true;
    }
    if (!doneX && near(z.right, r.right)) {
      z = { ...z, right: r.right, left: r.right - w };
      doneX = true;
    }
    if (!isView && !doneX && near(z.left, r.right)) {
      z = { ...z, left: r.right, right: r.right + w };
      doneX = true;
    }
    if (!doneY && near(z.bottom, r.bottom)) {
      z = { ...z, bottom: r.bottom, top: r.bottom - h };
      doneY = true;
    }
    if (!isView && !doneY && near(z.top, r.bottom)) {
      z = { ...z, top: r.bottom, bottom: r.bottom + h };
      doneY = true;
    }
    if ((!isView && noIntersect && intersects(z, r)) || !touches(z, r)) {
      z = oz;
      doneX = doneY = false;
    }
    if (doneX || doneY) snapped = true;
  }
  return { z, snapped };
}

/**
 * The docking of one modern skin's page.
 * @param {{
 *   containers: () => any[],
 *   rectOf: (c: any) => DOMRect | null,
 *   workAreaOf: (r: Rect) => Rect | null,
 *   appWindows: () => Promise<[string, [number, number, number, number]][]>,
 *   moveAppWindows: (moves: [string, number, number][]) => Promise<unknown>,
 * }} page
 */
export function createDocking(page) {
  /** Spotiamp+'s own windows, as last asked (page pixels), and when @type {[string, [number, number, number, number]][]} */
  let apps = [];
  let appsAt = 0;
  /** @type {Promise<void> | null} */
  let asking = null;
  function refreshApps() {
    asking ??= page
      .appWindows()
      .then((list) => {
        apps = Array.isArray(list) ? list : [];
        appsAt = performance.now();
      })
      .catch(() => {})
      .finally(() => (asking = null));
    return asking;
  }

  /** where a skin window docks: its layout's box less its snapAdjust (scaled as it's drawn) @param {any} c @returns {Rect | null} */
  function dockRect(c) {
    const r = page.rectOf(c);
    if (!r) return null;
    const layout = c._activeLayout;
    const s = layout?._snap ?? { left: 0, top: 0, right: 0, bottom: 0 };
    const k = layout?.getDiv()?.offsetWidth > 0 ? r.width / layout.getDiv().offsetWidth : 1;
    return { left: r.left + s.left * k, top: r.top + s.top * k, right: r.right - s.right * k, bottom: r.bottom - s.bottom * k };
  }

  /** the skin's windows there are to dock to, but one @param {any} [except] @returns {Win[]} */
  function skinWindows(except) {
    /** @type {Win[]} */
    const list = [];
    for (const c of page.containers()) {
      if (c === except || c._activeLayout?._noDock) continue;
      const rect = dockRect(c);
      if (rect) list.push({ kind: "skin", c, rect, x0: c._x, y0: c._y });
    }
    return list;
  }
  /** ...and Spotiamp+'s (none when what's known of them is old) @param {boolean} [fresh] @returns {Win[]} */
  function appWindows(fresh = false) {
    if (fresh && performance.now() - appsAt > 1000) return [];
    /** @type {Win[]} */
    const list = [];
    for (const [label, [x, y, w, h]] of apps) {
      if (w > 0 && h > 0) list.push({ kind: "app", label, rect: { left: x, top: y, right: x + w, bottom: y + h } });
    }
    return list;
  }

  // ---- a drag ----

  /** the dragged one, where it and the skin's other windows were when it started, and its group
   *  @type {null | { c: any, startX: number, startY: number, start: Rect, skins: Win[], followers: Win[], others: Win[], alt: boolean }} */
  let drag = null;
  let shift = false;
  let alt = false;
  /** Shift and Alt as the mouse last told them @param {MouseEvent} e */
  const keys = (e) => {
    shift = e.shiftKey;
    alt = e.altKey;
  };
  document.addEventListener("mousedown", keys, true);
  document.addEventListener("mousemove", keys, true);

  /** Wasabi's addCooperative: the windows docked to `from`, and theirs @param {Win} from @param {Win[]} pool @param {Win[]} out */
  function gather(from, pool, out) {
    for (const w of pool) {
      if (out.includes(w)) continue;
      if (touches(w.rect, from.rect) && !intersects(w.rect, from.rect)) {
        out.push(w);
        gather(w, pool, out);
      }
    }
  }

  /** the dragged one's group, from where they all were when it started
   *  (Spotiamp+'s windows only once known as they are) @param {boolean} fresh */
  function sortOut(fresh) {
    if (!drag) return;
    const pool = [...drag.skins, ...appWindows(fresh)];
    /** @type {Win[]} */
    let followers = [];
    if (!shift && (drag.alt || drag.c.getId() === "main")) {
      if (drag.alt) followers = pool.slice();
      else gather({ kind: "skin", c: drag.c, rect: drag.start, x0: drag.startX, y0: drag.startY }, pool, followers);
    }
    drag.followers = followers;
    drag.others = pool.filter((w) => !followers.includes(w));
  }

  /** @param {any} c */
  function start(c) {
    const rect = dockRect(c);
    drag = rect ? { c, startX: c._x, startY: c._y, start: rect, skins: skinWindows(c), followers: [], others: [], alt } : null;
    sortOut(true);
    // Spotiamp+'s windows as they are now (they may have moved since last asked)
    const was = drag;
    refreshApps().then(() => {
      if (drag && drag === was) sortOut(false);
    });
  }

  /** @param {any} c @param {number} x @param {number} y */
  function move(c, x, y) {
    if (!drag || drag.c !== c) return { x, y };
    let z = offset(drag.start, x - drag.startX, y - drag.startY);
    if (!shift) {
      const others = drag.others.map((w) => w.rect);
      z = autoDock(z, page.workAreaOf(z), others, false).z;
      // as Wasabi: one of the group docking somewhere moves all of it there
      let dx = z.left - drag.start.left;
      let dy = z.top - drag.start.top;
      let rx = 0;
      let ry = 0;
      for (const f of drag.followers) {
        const at = offset(f.rect, dx, dy);
        const { z: docked, snapped } = autoDock({ ...at }, page.workAreaOf(at), others, true);
        if (!snapped) continue;
        if (!rx) rx = docked.left - at.left;
        if (!ry) ry = docked.top - at.top;
      }
      if (rx || ry) z = offset(z, rx, ry);
    }
    const dx = Math.round(z.left - drag.start.left);
    const dy = Math.round(z.top - drag.start.top);
    follow(dx, dy);
    return { x: drag.startX + dx, y: drag.startY + dy };
  }

  /** the group where the dragged one now is, dx, dy from where they started @param {number} dx @param {number} dy */
  function follow(dx, dy) {
    if (!drag) return;
    /** @type {[string, number, number][]} */
    const moves = [];
    for (const f of drag.followers) {
      if (f.kind === "skin") {
        f.c.setLocation(f.x0 + dx, f.y0 + dy);
      } else {
        moves.push([f.label, Math.round(f.rect.left + dx), Math.round(f.rect.top + dy)]);
      }
    }
    if (moves.length) sendMoves(moves);
  }

  // Spotiamp+'s windows moved one batch at a time, the newest wins
  /** @type {[string, number, number][] | null} */
  let pending = null;
  let sending = false;
  /** @param {[string, number, number][]} moves */
  function sendMoves(moves) {
    pending = moves;
    if (sending) return;
    sending = true;
    const next = () => {
      const batch = pending;
      pending = null;
      if (!batch) {
        sending = false;
        return;
      }
      page
        .moveAppWindows(batch)
        .catch(() => {})
        .finally(next);
    };
    next();
  }

  function end() {
    drag = null;
    setTimeout(refreshApps, 300);
  }

  // ---- a window being sized (Wasabi's resize.cpp: autoDock of its moving edges) ----

  /** @param {any} c @param {Rect} outline the window as it's being sized @param {{ left: boolean, top: boolean, right: boolean, bottom: boolean }} way */
  function resize(c, outline, way) {
    if (shift) return outline;
    const layout = c._activeLayout;
    const s = layout?._snap ?? { left: 0, top: 0, right: 0, bottom: 0 };
    const box = page.rectOf(c);
    const k = box && layout?.getDiv()?.offsetWidth > 0 ? box.width / layout.getDiv().offsetWidth : 1;
    const inset = { left: s.left * k, top: s.top * k, right: s.right * k, bottom: s.bottom * k };
    const z = { left: outline.left + inset.left, top: outline.top + inset.top, right: outline.right - inset.right, bottom: outline.bottom - inset.bottom };
    const view = page.workAreaOf(z);
    const others = [...skinWindows(c), ...appWindows(true)].map((w) => w.rect);
    /** the nearest edge within the docking distance, of the screen's (its own side) or a window's that lies along this one @param {number} at @param {"x" | "y"} axis @param {"left" | "top" | "right" | "bottom"} side */
    const dock = (at, axis, side) => {
      let best = at;
      let bestD = DOCK_DIST;
      /** @param {number} edge */
      const consider = (edge) => {
        const d = Math.abs(edge - at);
        if (d < bestD) {
          bestD = d;
          best = edge;
        }
      };
      if (view) consider(view[side]);
      for (const r of others) {
        const along = axis === "x" ? r.bottom >= z.top && r.top <= z.bottom : r.right >= z.left && r.left <= z.right;
        if (!along) continue;
        if (axis === "x") {
          consider(r.left);
          consider(r.right);
        } else {
          consider(r.top);
          consider(r.bottom);
        }
      }
      return best;
    };
    if (way.left) z.left = dock(z.left, "x", "left");
    if (way.right) z.right = dock(z.right, "x", "right");
    if (way.top) z.top = dock(z.top, "y", "top");
    if (way.bottom) z.bottom = dock(z.bottom, "y", "bottom");
    return { left: z.left - inset.left, top: z.top - inset.top, right: z.right + inset.right, bottom: z.bottom + inset.bottom };
  }

  // ---- a size change (Wasabi's beforeRedock / afterRedock) ----

  /** @type {Map<any, { rect: Rect, bottom: Win[], top: Win[], right: Win[], left: Win[] }>} */
  const redocks = new Map();

  /** the windows docked to each side of `rect`, and to theirs along that line @param {any} c @param {Rect} rect */
  function sides(c, rect) {
    const pool = [...skinWindows(c), ...appWindows(true)];
    const found = { bottom: /** @type {Win[]} */ ([]), top: /** @type {Win[]} */ ([]), right: /** @type {Win[]} */ ([]), left: /** @type {Win[]} */ ([]) };
    const seen = new Set();
    /** @param {Rect} r1 @param {number} v -1 all sides, 1 up and down, 0 left and right */
    const walk = (r1, v) => {
      for (const w of pool) {
        const r2 = w.rect;
        const acrossX = r2.right >= r1.left - SLACK && r2.left <= r1.right + SLACK;
        const acrossY = r2.bottom >= r1.top - SLACK && r2.top <= r1.bottom + SLACK;
        /** @param {Win[]} list @param {number} nextV */
        const add = (list, nextV) => {
          if (seen.has(w)) return;
          seen.add(w);
          list.push(w);
          walk(r2, nextV);
        };
        if ((v === 1 || v === -1) && same(r2.top, r1.bottom) && acrossX) add(found.bottom, 1);
        else if ((v === 0 || v === -1) && same(r2.left, r1.right) && acrossY) add(found.right, 0);
        else if ((v === 0 || v === -1) && same(r2.right, r1.left) && acrossY) add(found.left, 0);
        else if ((v === 1 || v === -1) && same(r2.bottom, r1.top) && acrossX) add(found.top, 1);
      }
    };
    walk(rect, -1);
    return found;
  }

  /** @param {any} c */
  function beforeRedock(c) {
    // (a press on the window's button that switches it may have started a
    // drag that hasn't moved: the windows docked to it still follow)
    if (drag && drag.c === c && (c._x !== drag.startX || c._y !== drag.startY)) return;
    const rect = dockRect(c);
    if (rect) redocks.set(c, { rect, ...sides(c, rect) });
  }

  /** @param {any} c */
  function redock(c) {
    const was = redocks.get(c);
    redocks.delete(c);
    if (!was) return;
    const now = dockRect(c);
    if (!now) return;
    /** @type {[string, number, number][]} */
    const moves = [];
    /** @param {Win[]} list @param {number} dx @param {number} dy */
    const shiftAll = (list, dx, dy) => {
      if (!dx && !dy) return;
      for (const w of list) {
        if (w.kind === "skin") w.c.setLocation(Math.round(w.c._x + dx), Math.round(w.c._y + dy));
        else moves.push([w.label, Math.round(w.rect.left + dx), Math.round(w.rect.top + dy)]);
      }
    };
    shiftAll(was.bottom, 0, now.bottom - was.rect.bottom);
    shiftAll(was.top, 0, now.top - was.rect.top);
    shiftAll(was.right, now.right - was.rect.right, 0);
    shiftAll(was.left, now.left - was.rect.left, 0);
    if (moves.length) sendMoves(moves);
    if (moves.length) setTimeout(refreshApps, 300);
  }

  return { start, move, end, resize, beforeRedock, redock, refreshApps, dockRect };
}
