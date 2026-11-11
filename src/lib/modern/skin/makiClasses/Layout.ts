// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Group from "./Group";
import * as Utils from "../../utils";
import Container from "./Container";
import { LEFT, RIGHT, TOP, BOTTOM, CURSOR, MOVE } from "../Cursor";
import { px, unimplemented } from "../../utils";
import { UIRoot } from "../../UIRoot";
import PopupMenu from "./PopupMenu";
import { forEachMenuItem, IMenuItem } from "./MenuItem";
import { findAction } from "./menuWa5actions";

// > A layout is a special kind of group, which shown inside a container. Each
// > layout represents an appearance for that window. Layouts give you the ability
// > to design different looks for the same container (or window). However, only
// > one layout can be visible at a time. You must toggle among layouts you
// > defined. An example is the normal mode and windowshade mode in the Default
// > skin.
//
// -- http://wiki.winamp.com/wiki/Modern_Skin:_Container
// Parts of a window that take a press themselves: pressing one doesn't move
// the window (tags are the engine's class names, GuiObj.getElTag; "pl" is the
// playlist editor's box, PlayListGui).
const DRAG_CONTROLS = new Set([
  "button", "togglebutton", "nstatebutton", "wasabibutton", "slider", "edit", "input", "textarea", "select",
  "guilist", "guitree", "dropdownlist", "checkbox", "browser", "querylist", "vis", "avs", "eqvis",
  "windowholder", "componentbucket", "wrapper", "menu", "ul", "li",
]);

export default class Layout extends Group {
  static GUID = "60906d4e482e537e94cc04b072568861";
  _resizingDiv: HTMLDivElement = null;
  _resizing: boolean = false;
  _resizing_start: DOMRect = null;
  _canResize: number = 0; // combination of 4 directions: N/E/W/S
  _scale: number = 1.0;
  _opacity: number = 1.0;
  _desktopalpha: boolean = false;
  _movingStartX: number; //container XY
  _movingStartY: number;
  _moving: boolean = false;
  _snap = { left: 0, top: 0, right: 0, bottom: 0 };
  _shortcuts: { [key: string]: number } = {};

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._isLayout = true;
  }

  // Spotiamp+: as Wasabi (GuiObjectI::guiobject_onLeftButtonDown, and a
  // layout is a mover unless it says move="0"): a window is moved by dragging
  // it anywhere but on one of its controls. Its groups, layers without a
  // press of their own, grids and gui objects (Bento's mouse traps and title
  // bars) let the press through to it here.
  _moveOff: boolean = false;
  _dragWired: boolean = false;
  // (the layout's own handler below, not Movable's, which would move it from
  // a button too)
  _registerMovingEvents() {}
  _unregisterMovingEvents() {}

  draw() {
    super.draw();
    this._div.style.pointerEvents = "auto";
    if (!this._dragWired) {
      this._dragWired = true;
      this._div.addEventListener("mousedown", this._dragAnywhere);
    }
  }

  _dragAnywhere = (e: MouseEvent) => {
    // (one with move="1" is moved by GuiObj's own press, as any mover)
    if (e.button !== 0 || this._moveOff || this._mover || e.defaultPrevented) return;
    for (let el = e.target as Element | null; el && el !== this._div; el = el.parentElement) {
      if (DRAG_CONTROLS.has(el.tagName.toLowerCase()) || el.classList?.contains("pl")) return;
    }
    this._handleMoving(e);
  };

  setXmlAttr(key: string, value: string): boolean {
    if (key.toLowerCase() === "move") this._moveOff = !Utils.toBool(value);
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    switch (key) {
      case "desktopalpha":
        this._desktopAlpha = Utils.toBool(value);
        break;
      default:
        return false;
    }
    return true;
  }

  _renderBackground() {
    super._renderBackground(); //set css
    if (this._background != null && this._w == 0 && this._h == 0) {
      const bitmap = this._uiRoot.getBitmap(this._background);
      if (bitmap != null) {
        this._w = bitmap.getWidth();
        this._h = bitmap.getHeight();
        this._renderSize();
      }
    }
  }

  getcontainer(): Container {
    return this._parent as unknown as Container;
  }

  // (Spotiamp+: a layout is a window of its own: no group holds it, as in
  // Winamp; its container is getContainer())
  getparent(): any {
    return null;
  }

  gettop(): number {
    return this._parent._y;
  }

  /**
   * Get the X position, in the screen, of the
   * left edge of the object.
   *
   * @ret The left edge's position (in screen coordinates).
   */
  getleft(): number {
    return this._parent._x;
  }

  /**
   * Resize the object to the desired size and position.
   *
   * @param  x   The X position where to anchor the object before resize.
   * @param  y   The Y position where to anchor the object before resize.
   * @param  w   The width you wish the object to have.
   * @param  h   The height you wish the object to have.
   */
  resize(x: number, y: number, w: number, h: number) {
    const container = this._parent;
    // Spotiamp+: moved, not given a spot of the skin's (only its XML does
    // that); a script sizing its window as it loads isn't placing it
    container.setLocation(Number(x) || 0, Number(y) || 0);

    this._w = w;
    this._h = h;
    this._renderDimensions();
  }

  dispatchAction(
    action: string,
    param: string | null,
    actionTarget: string | null
  ) {
    // TODO: Maybe this should move to the container?
    if (actionTarget != null) {
      const target = this.findobject(actionTarget);
      if (target != null) {
        target.handleAction(action, param, actionTarget);
      }
      return;
    }
    switch (action) {
      default:
        if (this._parent != null) {
          this._parent.dispatchAction(action, param, actionTarget);
        }
    }
  }

  snapadjust(left: number, top: number, right: number, bottom: number) {
    this._snap.left = left;
    this._snap.top = top;
    this._snap.right = right;
    this._snap.bottom = bottom;
  }

  beforeredock() {
    // TODO:
  }

  redock() {
    // TODO:
  }

  getsnapadjustbottom(): number {
    return unimplemented(100);
  }

  clienttoscreenh(h: number): number {
    return unimplemented(h);
  }

  islayoutanimationsafe(): boolean {
    return true;
  }
  istransparencysafe(): boolean {
    return true;
  }

  getscale(): number {
    return this._scale;
  }
  setscale(scalevalue: number) {
    this._scale = scalevalue;
    this.getDiv().style.transform = `scale(${this._scale})`;
  }

  setdesktopalpha(onoff: boolean) {
    this._desktopalpha = unimplemented(onoff);
  }
  getdesktopalpha(): boolean {
    return this._desktopalpha;
  }

  init() {
    super.init();
  }
  afterInited() {
    this._invalidateSize();
    this._uiRoot.vm.dispatch(this, "onstartup");
  }

  setResizing(cmd: string, dx: number, dy: number) {
    // (Spotiamp+: the outline is in screen pixels, the window drawn at k times
    // its own: the skin's limits are scaled to it, and the size it ends up
    // with scaled back, Ctrl+D or a screen at 125 %)
    const k = this._resizeScale || 1;
    const clampW = (w): number => {
      w = this._maximumWidth ? Math.min(w, this._maximumWidth * k) : w;
      w = this._minimumWidth ? Math.max(w, this._minimumWidth * k) : w;
      return w;
    };
    const clampH = (h): number => {
      h = this._maximumHeight ? Math.min(h, this._maximumHeight * k) : h;
      h = this._minimumHeight ? Math.max(h, this._minimumHeight * k) : h;
      return h;
    };
    // const container = this._parent;
    if (cmd == "constraint") {
      this._canResize = dx;
    } else if (cmd == "start") {
      this.bringtofront();
      const r = this._div.getBoundingClientRect();
      // r.x = container.getleft()
      // r.y = container.gettop()
      this._resizing_start = r;
      this._resizeScale = this._div.offsetWidth > 0 ? r.width / this._div.offsetWidth : 1;
      // this._resizing_o = r;
      this._resizing = true;
      this._resizingDiv = document.createElement("div");
      this._resizingDiv.className = "resizing";
      // this._resizingDiv.style.cssText = "position:fixed;";
      // this._resizingDiv.style.width = px(r.width);
      // this._resizingDiv.style.height = px(r.height);
      // this._resizingDiv.style.top = px(container.gettop());
      // this._resizingDiv.style.left = px(container.getleft());
      const { left, top, width, height } = r;
      this._resizingDiv.style.cssText = `
        width: ${px(width)};
        height: ${px(height)};
        left: ${px(left)};
        top: ${px(top)};
        `;
      // (Spotiamp+: the outline as reckoned; its box has a border, so its
      // offsetWidth is a few pixels more)
      this._resizingRect = { left, top, width, height };
      // this._div.appendChild(this._resizingDiv);
      document.body.appendChild(this._resizingDiv);
    } else if (dx == CURSOR && dy == CURSOR) {
      this._resizingDiv.style.cursor = cmd;
    } else if (cmd == "move") {
      if (!this._resizing) {
        return;
      }
      // console.log(`resizing dx:${dx} dy:${dy}`);
      let { left, top, width, height, right, bottom } = this._resizing_start;
      if (this._canResize & RIGHT) width = clampW(width + dx);
      if (this._canResize & BOTTOM) height = clampH(height + dy);

      if (this._canResize & LEFT) {
        width = clampW(width + -dx);
        let l = left + dx;
        if (l + width <= right) {
          left = l;
        } else {
          left = right - this._minimumWidth * k;
        }
      }
      if (this._canResize & TOP) {
        height = clampH(height + -dy);
        let t = top + dy;
        if (t + height <= bottom) {
          top = t;
        } else {
          top = bottom - this._minimumHeight * k;
        }
      }
      this._resizingDiv.style.cssText = `
        width: ${px(width)};
        height: ${px(height)};
        left: ${px(left)};
        top: ${px(top)};
      `;
      this._resizingRect = { left, top, width, height };
    } else if (cmd == "final") {
      if (!this._resizing) {
        return;
      }
      this._resizing = false;
      const rr = this._resizingRect;
      this.setXmlAttr("w", Math.round(rr.width / k).toString());
      this.setXmlAttr("h", Math.round(rr.height / k).toString());
      const container = this._parent;
      container.setXmlAttr("x", Math.round(rr.left).toString());
      container.setXmlAttr("y", Math.round(rr.top).toString());
      this._resizingDiv.remove();
      this._resizingDiv = null;
      this._invalidateSize();
    }
  }

  // MOVING THINGS =====================
  setMoving(cmd: string, dx: number, dy: number) {
    const container = this._parent as unknown as Container;
    if (cmd == "start") {
      this._moving = true;
      this._movingStartX = container._x;
      this._movingStartY = container._y;
      this.bringtofront();
    } else if (dx == CURSOR && dy == CURSOR) {
    } else if (cmd == "move") {
      if (!this._moving) {
        return;
      }
      container.setLocation(this._movingStartX + dx, this._movingStartY + dy);
    } else if (cmd == "final") {
      if (!this._moving) {
        return;
      }
      this._invalidateSize();
      this._moving = false;
    }
  }

  // MENU SHORTCUT HANDLER HERE ======================
  registerShortcuts(popup: PopupMenu) {
    forEachMenuItem(popup, (m: IMenuItem) => {
      if (m.shortcut) {
        this._shortcuts[m.shortcut] = m.id;
      }
    });
    // console.log('layout.shortcuts:', this._shortcuts)
  }

  executeShorcut(shortcut: string) {
    const menuId = this._shortcuts[shortcut];
    const action = findAction(menuId);
    // console.log('Layout:', this._name, 'executing shortcut:',shortcut, '@action.id:', menuId, '=:', action )
    const invalidateRequired = action.onExecute(this._uiRoot);
  }
}
