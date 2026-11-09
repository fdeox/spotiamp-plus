// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Group from "./Group";

export default abstract class XuiElement extends Group {
  __inited: boolean = false;

  _unhandledXuiParams: { key: string; value: string }[] = []; //https://github.com/captbaritone/webamp/pull/1161#discussion_r830527754
  // Spotiamp+: Wasabi's embed_xui: the object inside that this element is to
  // the skin (Big Bento's tabs are their button, its search box its edit):
  // its unknown params go on to it, and a script's variable of that object's
  // kind holds it (the VM, interpreter.ts)
  _embedXuiId: string = null;
  _embeddedXui = null;

  getElTag(): string {
    return "group";
  }

  setXmlAttr(_key: string, value: string): boolean {
    const lowerkey = _key.toLowerCase();
    if (super.setXmlAttr(lowerkey, value)) {
      return true;
    }
    switch (lowerkey) {
      // Wasabi's own (Group::setXuiParam), not the element's
      case "embed_xui":
        this._embedXuiId = value;
        return true;
      case "xuitag":
      case "inherit_group":
      case "inherit_content":
        return true;
    }
    this._unhandledXuiParams.push({ key: lowerkey, value });
    // Spotiamp+: as Wasabi: one set once its scripts run goes to them now
    if (this.__inited) {
      for (const systemObject of this._systemObjects) this._tellXuiParam(systemObject, lowerkey, value);
      this._embeddedXui?.setxmlparam(lowerkey, value);
    }
    return true;
  }

  _tellXuiParam(systemObject, key: string, value: string) {
    this._uiRoot.vm.dispatch(systemObject, "onsetxuiparam", [
      { type: "STRING", value: key },
      { type: "STRING", value: value },
    ]);
  }

  // Spotiamp+: as Wasabi (Group::startScripts): each script is told the
  // params right after it loads
  _scriptStarted(systemObject) {
    for (const { key, value } of this._unhandledXuiParams) this._tellXuiParam(systemObject, key, value);
  }

  init() {
    if (this.__inited) return;
    super.init();
    this.__inited = true;
    // as Wasabi (EmbeddedXuiObject::embeddedxui_onNewEmbeddedContent), once
    // its scripts have started
    if (this._embedXuiId) {
      const embedded = this.findobject(this._embedXuiId);
      if (embedded && embedded !== this) {
        this._embeddedXui = embedded;
        for (const { key, value } of this._unhandledXuiParams) embedded.setxmlparam(key, value);
      }
    }
  }

  getembeddedobject() {
    return this._embeddedXui;
  }
}
