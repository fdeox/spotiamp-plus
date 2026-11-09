// Whether this window is the one in front, for the classic windows'
// titlebars: Winamp lights only that one's and dims the others (its
// "dim inactive windows" setting, on from the start).
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Tells `onChange` whether the window is in front, now and whenever that
 * changes; returns what stops it.
 * @param {(active: boolean) => void} onChange
 */
export function watchWindowActive(onChange) {
  onChange(document.hasFocus());
  const win = getCurrentWindow();
  const stop = win.onFocusChanged(({ payload }) => onChange(payload));
  return () => {
    stop.then((unlisten) => unlisten()).catch(() => {});
  };
}
