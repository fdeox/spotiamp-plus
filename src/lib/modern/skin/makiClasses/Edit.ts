// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import GuiObj from "./GuiObj";
import { UIRoot } from "../../UIRoot";

// Spotiamp+: Wasabi's <edit> (api/skin/widgets/edit.cpp over EditWnd), a
// line of text to type in: Big Bento's "Search in Playlist" box and the like.
// As Winamp's: the skin's wasabi.edit colours, Windows' small sans serif,
// onEditUpdate as it changes, onIdleEditUpdate once typing rests 350 ms,
// onEnter on Enter, onAbort on Escape.
export default class Edit extends GuiObj {
  static GUID = "64e4bbfa49d981f45ba8c0b0fdbcc32e";
  _input: HTMLInputElement | HTMLTextAreaElement;
  _autoEnter: boolean = false;
  _autoSelect: boolean = false;
  _idleEnabled: boolean = true;
  _idleTimer: ReturnType<typeof setTimeout> | null = null;
  _multiline: boolean = false;
  _password: boolean = false;

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._makeInput();
  }

  getElTag(): string {
    return "edit";
  }

  _makeInput() {
    const old = this._input;
    const input = document.createElement(this._multiline ? "textarea" : "input");
    if (!this._multiline) (input as HTMLInputElement).type = this._password ? "password" : "text";
    input.spellcheck = false;
    input.autocomplete = "off";
    if (old) input.value = old.value;
    input.style.cssText =
      "position:absolute;inset:0;width:100%;height:100%;box-sizing:border-box;margin:0;padding:0 2px;border:0;outline:none;resize:none;" +
      "font:11px 'Microsoft Sans Serif',Tahoma,Arial,sans-serif;" +
      "color:var(--edit-text,#000);background:var(--edit-back,#fff);";
    input.addEventListener("input", () => this._changed());
    input.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === "Enter" && !(this._multiline && !e.ctrlKey)) {
        e.preventDefault();
        this._uiRoot.vm.dispatch(this, "onenter");
      } else if (e.key === "Escape") {
        e.preventDefault();
        this._uiRoot.vm.dispatch(this, "onabort");
      }
      // its keys are its own, not the skin's or the player's
      e.stopPropagation();
    });
    input.addEventListener("focus", () => {
      if (this._autoSelect) input.select();
      this._uiRoot.vm.dispatch(this, "onsetfocus");
    });
    input.addEventListener("blur", () => this._uiRoot.vm.dispatch(this, "onkillfocus"));
    // a click in it is a click in it, not a drag of the window
    input.addEventListener("pointerdown", (e) => e.stopPropagation());
    if (old) old.replaceWith(input);
    else this._div.appendChild(input);
    this._input = input;
    this._renderColors();
  }

  _changed() {
    this._uiRoot.vm.dispatch(this, "oneditupdate");
    if (this._idleTimer) clearTimeout(this._idleTimer);
    this._idleTimer = setTimeout(() => {
      this._idleTimer = null;
      if (this._idleEnabled) this._uiRoot.vm.dispatch(this, "onidleeditupdate");
    }, 350);
  }

  // the skin's colours for it (EditWnd), else its lists'
  _renderColors() {
    const pick = (ids: string[], fallback: string) => {
      for (const id of ids) {
        const color = this._uiRoot.getColor(id);
        if (color) return `var(${color.getCSSVar()}, ${color.getRgb()})`;
      }
      return fallback;
    };
    this._div.style.setProperty("--edit-text", pick(["wasabi.edit.text", "wasabi.list.text"], "#000"));
    this._div.style.setProperty("--edit-back", pick(["wasabi.edit.background", "wasabi.list.background"], "#fff"));
  }

  setXmlAttr(_key: string, value: string): boolean {
    const key = _key.toLowerCase();
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    switch (key) {
      case "text":
        this.settext(value);
        break;
      case "autoenter":
        this._autoEnter = value !== "0";
        break;
      case "autoselect":
        this._autoSelect = value !== "0";
        break;
      case "password":
        this._password = value !== "0";
        this._makeInput();
        break;
      case "multiline":
        this._multiline = value !== "0";
        this._makeInput();
        break;
      case "autohscroll":
      case "vscroll":
      case "action":
        break;
      default:
        return false;
    }
    return true;
  }

  draw() {
    super.draw();
    this._renderColors();
  }

  settext(txt: string) {
    this._input.value = txt ?? "";
  }
  gettext(): string {
    return this._input.value;
  }
  setautoenter(onoff: boolean) {
    this._autoEnter = !!onoff;
  }
  getautoenter(): boolean {
    return this._autoEnter;
  }
  setidleenabled(onoff: boolean) {
    this._idleEnabled = !!onoff;
  }
  getidleenabled(): boolean {
    return this._idleEnabled;
  }
  selectall() {
    this._input.select();
  }
  enter() {
    this._uiRoot.vm.dispatch(this, "onenter");
  }
  setfocus() {
    this._input.focus();
  }
}
