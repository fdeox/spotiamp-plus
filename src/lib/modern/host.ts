// Spotiamp+: where a modern skin's engine reaches the app. The engine (this
// folder, from Webamp Modern) knows nothing of Tauri; the modern window's
// page (routes/modern) fills these in, and the engine calls them for what
// Winamp itself would do: next and previous song, open files, its menus,
// close and minimize, and the windows Spotiamp+ has of its own.

const nothing = () => {};

/** A screen as modern_window.rs tells it, in the page's pixels. */
export type Screen = {
  x: number;
  y: number;
  width: number;
  height: number;
  work_x: number;
  work_y: number;
  work_width: number;
  work_height: number;
  scale: number;
  primary: boolean;
};

export const HOST = {
  next: nothing as () => void,
  previous: nothing as () => void,
  /** Winamp's eject: open files */
  eject: nothing as () => void,
  /** one of Winamp's menus was asked for (by name, as the skin names it) */
  menu: nothing as (name: string) => void,
  /** the main window's close: quits, as the classic one does */
  close: nothing as () => void,
  minimize: nothing as () => void,
  openVisualizer: nothing as () => void,
  /** a window the skin has no container for: Spotiamp+'s own, if it has one
   *  ("ml" is the Library, "vis" the visualizer); false if it doesn't */
  toggleWindow: ((id: string) => false) as (id: string) => boolean,
  /** the skin changed what's shown: its windows moved, opened or closed */
  layoutChanged: nothing as () => void,
  /** the screens (routes/modern fills it in) */
  screens: [] as Screen[],
  /** where the skin's main window is (page pixels), for "this screen" */
  mainSpot: (() => null) as () => { x: number; y: number } | null,
};

/**
 * Winamp's idea of a screen at a point (or the one the skin's main window
 * is on, else the main screen): its spot in the page's pixels and its size
 * in the skin's own pixels (a 125 % screen's 1920 is 1536 to a skin drawn a
 * quarter bigger there). The whole page lies over all the screens, so its
 * own size is no screen's.
 */
export function screenAt(x?: number, y?: number) {
  const list = HOST.screens;
  const at = (px: number, py: number) =>
    list.find((s) => px >= s.x && py >= s.y && px < s.x + s.width && py < s.y + s.height);
  let s = x != null && y != null ? at(x, y) : undefined;
  if (!s) {
    const main = HOST.mainSpot();
    if (main) s = at(main.x, main.y);
  }
  s = s ?? list.find((one) => one.primary) ?? list[0];
  if (!s) {
    return { left: 0, top: 0, width: screen.width, height: screen.height, workLeft: 0, workTop: 0, workWidth: screen.availWidth, workHeight: screen.availHeight };
  }
  const k = s.scale || 1;
  return {
    left: s.x,
    top: s.y,
    width: Math.round(s.width / k),
    height: Math.round(s.height / k),
    workLeft: s.work_x,
    workTop: s.work_y,
    workWidth: Math.round(s.work_width / k),
    workHeight: Math.round(s.work_height / k),
  };
}
