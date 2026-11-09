// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Group from "./Group";

// Spotiamp+: Wasabi:TitleBox, a group box with its title along the top
// (Wasabi's titlebox.cpp): three title parts, wasabi.titlebox.left.group,
// .center.group (holding the "titlebox.text", as wide as its text) and
// .right.group (the rest of the width), over wasabi.titlebox.main.group (the
// box), whose "titlebox.content" holds the group the skin names in content=.
// The parts are groupdefs: the skin's, else our Wasabi base's
// (SkinEngine_WAL.titleBox builds it).
export default class TitleBox extends Group {
  static GUID = "OFFICIALLY-NO-GUID";
  _title: string = "";
  _suffix: string = "";
  _contentId: string = "";
  _centered: boolean = false;
  _left: Group = null;
  _center: Group = null;
  _right: Group = null;
  _main: Group = null;
  _content: Group = null;

  // drawn as a group (and so named whatever a minifier calls the class)
  getElTag(): string {
    return "group";
  }

  setXmlAttr(_key: string, value: string): boolean {
    const key = _key.toLowerCase();
    switch (key) {
      case "title":
        this._title = value;
        this._renderTitle();
        return true;
      case "suffix":
        this._suffix = value;
        this._renderTitle();
        return true;
      case "content":
        this._contentId = value;
        return true;
      case "centered":
        this._centered = value !== "0";
        this._layoutTitle();
        return true;
    }
    return super.setXmlAttr(_key, value);
  }

  // the title: the one given, else the content's name
  _renderTitle() {
    const text = this._center?.findobject?.("titlebox.text");
    if (!text) return;
    const name = this._title || this._content?._name || this._content?.getname?.() || "";
    if (name) text.settext(name + this._suffix);
    this._layoutTitle();
  }

  // as Wasabi's onResize: the left part its own width (or what centres the
  // title), the title as wide as its text, the right part the rest
  _layoutTitle() {
    if (!this._left || !this._center || !this._right) return;
    const w = this.getwidth();
    const text = this._center.findobject("titlebox.text");
    const textW = text ? text.getautowidth() : 0;
    const leftW = this._centered ? Math.max(0, Math.round((w - textW) / 2)) : this._left._w || 0;
    const place = (part, x, pw) => {
      part.setxmlparam("relatw", "0");
      part.setxmlparam("x", String(x));
      part.setxmlparam("w", String(Math.max(0, pw)));
    };
    place(this._left, 0, leftW);
    place(this._center, leftW, textW);
    place(this._right, leftW + textW, w - leftW - textW);
  }

  init() {
    super.init();
    this._renderTitle();
  }

  doResize() {
    this._layoutTitle();
    super.doResize();
  }
}
