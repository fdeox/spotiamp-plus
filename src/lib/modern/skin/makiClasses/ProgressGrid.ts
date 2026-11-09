// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Grid from "./Grid";
import { UIRoot } from "../../UIRoot";
import { px } from "../../utils";

// Spotiamp+: as Wasabi's (xuiprogressgrid.cpp): the whole grid, its ends
// too, drawn in the part of its box that the song has played, from the side
// its orientation names; nothing while stopped
export default class ProgressGrid extends Grid {
  static GUID = "OFFICIALLY-NO-GUID";
  _disposeDisplaySubscription: Function;
  _disposeStateSubscription: Function;
  _orientation: string = "right";

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._disposeDisplaySubscription = this._uiRoot.audio.onCurrentTimeChange(() => this._renderProgress());
    this._disposeStateSubscription = this._uiRoot.audio.on("statchanged", () => this._renderProgress());
    // the grid shows only inside what's left once the unplayed part is padded off
    this._div.style.boxSizing = "border-box";
    this._div.style.overflow = "clip";
    this._div.style.overflowClipMargin = "content-box";
  }

  setXmlAttr(key: string, value: string): boolean {
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    switch (key) {
      case "orientation":
        this._orientation = String(value).toLowerCase();
        this._renderProgress();
        break;
      case "interval":
        break;
      default:
        return false;
    }
    return true;
  }

  _renderProgress() {
    const audio = this._uiRoot.audio;
    const p = audio.getState() === "stopped" ? 0 : Math.min(1, Math.max(0, audio.getCurrentTimePercent() || 0));
    const s = this._div.style;
    const vertical = this._orientation === "top" || this._orientation === "bottom";
    const size = vertical ? this.getheight() : this.getwidth();
    const cut = px(Math.round((size || 0) * (1 - p)));
    // nothing played: nothing drawn (its size may not be known yet)
    for (const part of [this._left, this._middle, this._right]) part.style.visibility = p > 0 ? "" : "hidden";
    s.padding = "0";
    if (this._orientation === "left") s.paddingLeft = cut;
    else if (this._orientation === "top") s.paddingTop = cut;
    else if (this._orientation === "bottom") s.paddingBottom = cut;
    else s.paddingRight = cut;
  }

  draw() {
    super.draw();
    // Spotiamp+: a hidden one stays hidden
    this._renderVisibility();
    this._renderProgress();
  }

  dispose() {
    this._disposeDisplaySubscription?.();
    this._disposeStateSubscription?.();
    super.dispose?.();
  }
}
