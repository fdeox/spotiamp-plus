import {
  getCurrentWindow,
  PhysicalPosition,
  Window,
} from "@tauri-apps/api/window";
import { emitWindowEvent } from "./events.svelte.js";

export const SNAP_DISTANCE = 10;
export const STICKY_SNAP_DISTANCE = 25;

/** Every window that participates in docking. */
export const DOCKABLE_LABELS = [
  "player",
  "playlist",
  "library",
  "visualizer",
  "eq",
  "lyrics",
];

/**
 * Rects of every open, visible window except `selfLabel` — so a dragged window
 * can magnetically snap to any of the others (not only the player).
 * @param {string} selfLabel
 * @returns {Promise<WindowRect[]>}
 */
export async function collectSnapRects(selfLabel) {
  const rects = [];
  await Promise.all(
    DOCKABLE_LABELS.filter((label) => label !== selfLabel).map(async (label) => {
      try {
        const win = await Window.getByLabel(label);
        if (!win || !(await win.isVisible())) return;
        const [position, size] = await Promise.all([
          win.outerPosition(),
          win.outerSize(),
        ]);
        rects.push(rectFromPositionAndSize(position, size));
      } catch (e) {
        /* window not open — skip */
      }
    }),
  );
  return rects;
}

/**
 * Snap a size so the window's right and bottom edges land exactly on a
 * neighbour's edge.
 *
 * Dragging has always snapped, but resizing did not, so growing a window meant
 * lining its edge up by eye and it would happily overlap whatever sat next to
 * it. The top-left corner is the fixed anchor while resizing, so only the far
 * edges need to be considered.
 *
 * @param {WindowRect} rect the window being resized, at its current size
 * @param {WindowRect[]} others rects to snap against
 * @param {number} [distance]
 * @returns {{ width: number, height: number }}
 */
export function snapSize(rect, others, distance = SNAP_DISTANCE) {
  /**
   * @param {number} start
   * @param {number} size
   * @param {number[]} edges
   */
  const snapEdge = (start, size, edges) => {
    let best = size;
    let bestDistance = distance;
    for (const edge of edges) {
      const d = Math.abs(start + size - edge);
      // `<=` so that with two equally close edges the later one wins, matching
      // how the drag snapping resolves ties.
      if (d <= bestDistance) {
        bestDistance = d;
        best = edge - start;
      }
    }
    return best;
  };

  const xEdges = others.flatMap((o) => [o.x, rectRight(o)]);
  const yEdges = others.flatMap((o) => [o.y, rectBottom(o)]);
  return {
    width: snapEdge(rect.x, rect.width, xEdges),
    height: snapEdge(rect.y, rect.height, yEdges),
  };
}

/**
 * Wire a resize grip so the window's far edges snap to its neighbours.
 *
 * Three coordinate systems meet here and getting any of them wrong shows up as
 * a window that looks snapped but sits a few pixels past its neighbour:
 *
 *   * sizes inside the app are logical, and double-size scales them again;
 *   * `setSize` sets the window's INNER size;
 *   * neighbour rects arrive as OUTER physical pixels.
 *
 * So the scale comes from `scaleFactor()` and the zoom directly rather than
 * being inferred from a ratio, and the frame thickness (outer minus inner) is
 * measured once and added before snapping, then taken back off afterwards.
 *
 * @param {HTMLElement} element the grip
 * @param {string} selfLabel
 * @param {(e: PointerEvent) => { width: number, height: number }} measure
 *   raw logical size from the pointer, before snapping
 * @param {(size: { width: number, height: number }) => void} apply
 * @param {() => number} zoom current double-size factor
 */
export function makeSnappingResizer(element, selfLabel, measure, apply, zoom) {
  element.onpointerdown = function (event) {
    event.preventDefault();
    element.setPointerCapture(event.pointerId);

    /** @type {WindowRect[]} */
    let rects = [];
    let origin = { x: 0, y: 0 };
    let scale = 1;
    let frameW = 0;
    let frameH = 0;
    // Gathered once per gesture: querying every window on each pointermove
    // would hammer the window API for numbers that cannot change mid-drag.
    (async () => {
      try {
        const win = getCurrentWindow();
        const [collected, position, outer, inner, factor] = await Promise.all([
          collectSnapRects(selfLabel),
          win.outerPosition(),
          win.outerSize(),
          win.innerSize(),
          win.scaleFactor(),
        ]);
        rects = collected;
        origin = { x: position.x, y: position.y };
        scale = factor * (zoom() || 1);
        frameW = outer.width - inner.width;
        frameH = outer.height - inner.height;
      } catch {
        /* no neighbours to snap to — resizing still works, just freely */
      }
    })();

    document.onpointermove = function (e) {
      const raw = measure(e);
      // Snap the OUTER edges, because that is what the neighbours' rects
      // describe; the inner size we actually set is derived back from it.
      const snapped = snapSize(
        {
          x: origin.x,
          y: origin.y,
          width: raw.width * scale + frameW,
          height: raw.height * scale + frameH,
        },
        rects,
      );
      apply({
        width: (snapped.width - frameW) / scale,
        height: (snapped.height - frameH) / scale,
      });
    };
    document.onpointerup = function () {
      document.onpointermove = null;
      element.releasePointerCapture(event.pointerId);
    };
  };
  element.onselectstart = () => false;
}

/**
 * Wire a window's titlebar so it snaps to ANY other window while dragging and
 * broadcasts its drag on `eventName` (drives the native player-anchored dock).
 * @param {HTMLElement} element
 * @param {string} selfLabel
 * @param {string} eventName
 */
export function makeDockedDraggable(element, selfLabel, eventName) {
  makeTauriWindowDraggable(element, {
    async onStart({ startPosition, windowSize }) {
      await emitWindowEvent(eventName, { DragStarted: null });
      const snapRects = await collectSnapRects(selfLabel);
      const startRect = rectFromPositionAndSize(startPosition, windowSize);
      return {
        snapRects,
        windowSize,
        docked: snapRects.some((rect) => isDocked(startRect, rect)),
      };
    },
    mapPosition(rawPosition, context) {
      const rawRect = {
        ...rawPosition,
        width: context.windowSize.width,
        height: context.windowSize.height,
      };
      const snapDistance = context.docked
        ? STICKY_SNAP_DISTANCE
        : SNAP_DISTANCE;
      // Snap to the CLOSEST window in range, not the first one that happens to
      // be near — otherwise a window sitting between two others could grab the
      // wrong neighbour (e.g. the library jumping onto the visualizer instead
      // of docking to the playlist it was next to).
      let best;
      for (const rect of context.snapRects) {
        const result = snapPosition(rawRect, rect, snapDistance);
        if (result && (!best || result.distance < best.distance)) {
          best = result;
        }
      }
      context.docked = best !== undefined;
      return best?.position ?? rawPosition;
    },
    async onEnd() {
      await emitWindowEvent(eventName, { DragEnded: null });
    },
  });
}

/**
 * @typedef {{ x: number, y: number, width: number, height: number }} WindowRect
 */

/**
 * @typedef {{ x: number, y: number }} WindowPosition
 */

/**
 * @typedef {{
 *   currentWindow: ReturnType<typeof getCurrentWindow>,
 *   startPosition: WindowPosition,
 *   windowSize: { width: number, height: number },
 *   scaleFactor: number,
 *   pointerDownEvent: PointerEvent
 * }} WindowDragStart
 */

/**
 * @param {WindowRect} rect
 */
export function rectRight(rect) {
  return rect.x + rect.width;
}

/**
 * @param {WindowRect} rect
 */
export function rectBottom(rect) {
  return rect.y + rect.height;
}

/**
 * @param {WindowPosition} position
 * @param {{ width: number, height: number }} size
 */
export function rectFromPositionAndSize(position, size) {
  return {
    x: position.x,
    y: position.y,
    width: size.width,
    height: size.height,
  };
}

/**
 * @param {number} aStart
 * @param {number} aEnd
 * @param {number} bStart
 * @param {number} bEnd
 */
function rangesOverlap(aStart, aEnd, bStart, bEnd) {
  return aStart < bEnd && bStart < aEnd;
}

/**
 * @param {number} aStart
 * @param {number} aEnd
 * @param {number} bStart
 * @param {number} bEnd
 */
function rangesTouch(aStart, aEnd, bStart, bEnd) {
  return aStart <= bEnd && bStart <= aEnd;
}

/**
 * @param {{x: number, y: number}} position
 * @param {WindowRect} windowRect
 * @param {WindowRect} otherRect
 * @param {'x' | 'y'} axis
 * @param {number} snapDistance
 */
function snapToGuide(position, windowRect, otherRect, axis, snapDistance) {
  const size = axis == "x" ? windowRect.width : windowRect.height;
  const otherStart = otherRect[axis];
  const otherEnd = axis == "x" ? rectRight(otherRect) : rectBottom(otherRect);
  const ownStart = position[axis];
  const ownEnd = position[axis] + size;

  const candidates = [
    { distance: Math.abs(ownStart - otherStart), value: otherStart },
    { distance: Math.abs(ownStart - otherEnd), value: otherEnd },
    { distance: Math.abs(ownEnd - otherStart), value: otherStart - size },
    { distance: Math.abs(ownEnd - otherEnd), value: otherEnd - size },
  ].sort((a, b) => a.distance - b.distance);

  return candidates[0].distance <= snapDistance
    ? candidates[0].value
    : position[axis];
}

/**
 * @param {WindowRect} windowRect
 * @param {WindowRect} otherRect
 * @param {'x' | 'y'} axis
 * @param {number} snapDistance
 */
function overlapsOrNearGuide(windowRect, otherRect, axis, snapDistance) {
  const start = windowRect[axis];
  const end = axis == "x" ? rectRight(windowRect) : rectBottom(windowRect);
  const otherStart = otherRect[axis];
  const otherEnd = axis == "x" ? rectRight(otherRect) : rectBottom(otherRect);

  return (
    rangesOverlap(start, end, otherStart, otherEnd) ||
    Math.abs(start - otherStart) <= snapDistance ||
    Math.abs(start - otherEnd) <= snapDistance ||
    Math.abs(end - otherStart) <= snapDistance ||
    Math.abs(end - otherEnd) <= snapDistance
  );
}

/**
 * @param {WindowRect} windowRect
 * @param {WindowRect} otherRect
 * @param {number} snapDistance
 */
export function snapPosition(windowRect, otherRect, snapDistance) {
  const candidates = [
    {
      distance: Math.abs(rectRight(windowRect) - otherRect.x),
      snaps: overlapsOrNearGuide(windowRect, otherRect, "y", snapDistance),
      position: {
        x: otherRect.x - windowRect.width,
        y: snapToGuide(
          { x: windowRect.x, y: windowRect.y },
          windowRect,
          otherRect,
          "y",
          snapDistance,
        ),
      },
    },
    {
      distance: Math.abs(windowRect.x - rectRight(otherRect)),
      snaps: overlapsOrNearGuide(windowRect, otherRect, "y", snapDistance),
      position: {
        x: rectRight(otherRect),
        y: snapToGuide(
          { x: windowRect.x, y: windowRect.y },
          windowRect,
          otherRect,
          "y",
          snapDistance,
        ),
      },
    },
    {
      distance: Math.abs(rectBottom(windowRect) - otherRect.y),
      snaps: overlapsOrNearGuide(windowRect, otherRect, "x", snapDistance),
      position: {
        x: snapToGuide(
          { x: windowRect.x, y: windowRect.y },
          windowRect,
          otherRect,
          "x",
          snapDistance,
        ),
        y: otherRect.y - windowRect.height,
      },
    },
    {
      distance: Math.abs(windowRect.y - rectBottom(otherRect)),
      snaps: overlapsOrNearGuide(windowRect, otherRect, "x", snapDistance),
      position: {
        x: snapToGuide(
          { x: windowRect.x, y: windowRect.y },
          windowRect,
          otherRect,
          "x",
          snapDistance,
        ),
        y: rectBottom(otherRect),
      },
    },
  ].sort((a, b) => a.distance - b.distance);

  const candidate = candidates[0];
  // Returns the snapped position *and* how close the snap was, so a caller
  // choosing among several windows can pick the nearest one rather than the
  // first that happens to be in range.
  return candidate.snaps && candidate.distance <= snapDistance
    ? { position: candidate.position, distance: candidate.distance }
    : undefined;
}

/**
 * @param {WindowRect[]} rects
 */
export function boundingRect(rects) {
  const x = Math.min(...rects.map((rect) => rect.x));
  const y = Math.min(...rects.map((rect) => rect.y));
  return {
    x,
    y,
    width: Math.max(...rects.map(rectRight)) - x,
    height: Math.max(...rects.map(rectBottom)) - y,
  };
}

/**
 * @param {WindowRect} windowRect
 * @param {WindowRect} boundsRect
 * @param {number} snapDistance
 */
export function snapRectIntoBounds(windowRect, boundsRect, snapDistance) {
  let x = windowRect.x;
  let y = windowRect.y;
  let snapped = false;
  const xCandidates = [
    { distance: Math.abs(windowRect.x - boundsRect.x), value: boundsRect.x },
    {
      distance: Math.abs(rectRight(windowRect) - rectRight(boundsRect)),
      value: rectRight(boundsRect) - windowRect.width,
    },
  ].sort((a, b) => a.distance - b.distance);
  const yCandidates = [
    { distance: Math.abs(windowRect.y - boundsRect.y), value: boundsRect.y },
    {
      distance: Math.abs(rectBottom(windowRect) - rectBottom(boundsRect)),
      value: rectBottom(boundsRect) - windowRect.height,
    },
  ].sort((a, b) => a.distance - b.distance);

  if (xCandidates[0].distance <= snapDistance) {
    x = xCandidates[0].value;
    snapped = true;
  }
  if (yCandidates[0].distance <= snapDistance) {
    y = yCandidates[0].value;
    snapped = true;
  }

  return snapped ? { x, y } : undefined;
}

/**
 * @param {WindowRect} windowRect
 * @param {WindowRect} otherRect
 */
export function isDocked(windowRect, otherRect) {
  const verticallyTouches = rangesTouch(
    windowRect.y,
    rectBottom(windowRect),
    otherRect.y,
    rectBottom(otherRect),
  );
  const horizontallyTouches = rangesTouch(
    windowRect.x,
    rectRight(windowRect),
    otherRect.x,
    rectRight(otherRect),
  );

  return (
    (verticallyTouches && rectRight(windowRect) == otherRect.x) ||
    (verticallyTouches && windowRect.x == rectRight(otherRect)) ||
    (horizontallyTouches && rectBottom(windowRect) == otherRect.y) ||
    (horizontallyTouches && windowRect.y == rectBottom(otherRect))
  );
}

/**
 * @template T
 * @param {HTMLElement} element
 * @param {{
 *   onStart?: (drag: WindowDragStart) => Promise<T | false> | T | false,
 *   mapPosition?: (rawPosition: WindowPosition, context: T, drag: WindowDragStart) => WindowPosition,
 *   onEnd?: (context: T, drag: WindowDragStart) => Promise<void> | void,
 * }} options
 */
export function makeTauriWindowDraggable(element, options = {}) {
  element.onpointerdown = async function (event) {
    // Don't start a drag when the press lands on an interactive control inside
    // the drag handle (e.g. a titlebar close button). Svelte 5 delegates
    // pointerdown, so a child's stopPropagation runs *after* this direct
    // handler — hence this explicit opt-out instead.
    if (event.target instanceof Element && event.target.closest("[data-no-drag]")) {
      return;
    }
    event.preventDefault();

    const currentWindow = getCurrentWindow();
    const scaleFactor = await currentWindow.scaleFactor();
    const [startPosition, windowSize] = await Promise.all([
      currentWindow.outerPosition(),
      currentWindow.outerSize(),
    ]);
    const drag = {
      currentWindow,
      startPosition,
      windowSize,
      scaleFactor,
      pointerDownEvent: event,
    };
    const context = await options.onStart?.(drag);
    if (context === false) {
      return;
    }
    const startPointer = {
      x: event.screenX * scaleFactor,
      y: event.screenY * scaleFactor,
    };
    /**
     * @type {WindowPosition | undefined}
     */
    let nextPosition;
    let moving = false;

    async function applyNextPosition() {
      if (moving || !nextPosition) {
        return;
      }

      moving = true;
      const position = nextPosition;
      nextPosition = undefined;
      await currentWindow.setPosition(
        new PhysicalPosition(position.x, position.y),
      );
      moving = false;
      await applyNextPosition();
    }

    async function settlePosition() {
      while (moving || nextPosition) {
        await applyNextPosition();
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
    }

    /**
     * @param {PointerEvent} event
     */
    element.onpointermove = async function (event) {
      const rawPosition = {
        x: Math.round(
          startPosition.x + event.screenX * scaleFactor - startPointer.x,
        ),
        y: Math.round(
          startPosition.y + event.screenY * scaleFactor - startPointer.y,
        ),
      };

      nextPosition =
        options.mapPosition?.(
          rawPosition,
          /** @type {T} */ (context),
          drag,
        ) ?? rawPosition;
      await applyNextPosition();
    };

    element.onpointerup = async function () {
      element.onpointermove = null;
      element.onpointerup = null;
      element.onpointercancel = null;
      await settlePosition();
      await options.onEnd?.(/** @type {T} */ (context), drag);
      element.releasePointerCapture(event.pointerId);
    };
    element.onpointercancel = element.onpointerup;

    element.setPointerCapture(event.pointerId);
  };

  element.onselectstart = () => false;
}
