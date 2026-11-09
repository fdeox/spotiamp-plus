// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Group from "./Group";

/**
 * Spotiamp+: Wasabi:TabSheet as Wasabi laid it out (tabsheet.cpp,
 * tabsheetbar.cpp): its pages are the groups its "children" name, one shown
 * at a time; a tab for each across the top, from the skin's (or the base's)
 * wasabi.tabsheet.button.selected / unselected groups with the page's name
 * on them, as wide as their text wants, 4 px over each other, the selected
 * one in front; wasabi.tabsheet.nobutton.group along the rest of the row;
 * the page in wasabi.tabsheet.content below, in its "content" part. Read for
 * its behaviour; none of Winamp's code is used.
 */
export default class TabSheet extends Group {
  static GUID = "b5baa5354dcb05b318e6c1ad96688fd2";
  _tabs: { name: string; page: any; on: any; off: any }[] = [];
  _content: any = null;
  _bottom: any = null;
  _current: number = 0;
  /** the tabs' row height (the tab groups' h) */
  _barH: number = 0;
  _resize: ResizeObserver | null = null;

  getElTag(): string {
    return "group";
  }

  draw() {
    super.draw();
    this._resize = new ResizeObserver(() => this._layOut());
    this._resize.observe(this._div);
  }

  init() {
    super.init();
    for (const [i, tab] of this._tabs.entries()) {
      for (const button of [tab.on, tab.off]) {
        button?.findobject?.("text")?.settext?.(tab.name);
        const trap = button?.findobject?.("mousetrap") ?? null;
        const div = (trap ?? button)?.getDiv?.();
        if (div) {
          div.style.pointerEvents = "auto";
          div.addEventListener("click", () => this.setcurpage(i));
        }
      }
    }
    this.setcurpage(this._current);
    // (once the tabs' texts have measured themselves)
    for (const ms of [0, 300, 1000, 2500]) setTimeout(() => this._layOut(), ms);
  }

  dispose() {
    this._resize?.disconnect();
    super.dispose?.();
  }

  getcurpage(): number {
    return this._current;
  }

  getnumpages(): number {
    return this._tabs.length;
  }

  setcurpage(page: number) {
    if (page < 0 || page >= this._tabs.length) return;
    this._current = page;
    this._tabs.forEach((tab, i) => {
      const on = i === page;
      if (on) tab.page.show?.();
      else tab.page.hide?.();
      if (on) tab.on?.show?.();
      else tab.on?.hide?.();
      if (on) tab.off?.hide?.();
      else tab.off?.show?.();
    });
    this._layOut();
  }

  nextpage() {
    this.setcurpage((this._current + 1) % Math.max(1, this._tabs.length));
  }

  previouspage() {
    this.setcurpage((this._current - 1 + this._tabs.length) % Math.max(1, this._tabs.length));
  }

  /** the tabs along the top, the row's rest, the content and the page */
  _layOut() {
    const w = Math.round(this._div?.offsetWidth ?? 0);
    const h = Math.round(this._div?.offsetHeight ?? 0);
    if (w < 1 || h < 1) return;
    const groupH = (g) => Number(g?._h) || 0;
    // a tab's width: its text's, with the room the group leaves around it
    // (66 without a text)
    const tabW = (g) => {
      const text = g?.findobject?.("text");
      if (!text) return 66;
      const around = text._relatw == "1" ? -(Number(text._w) || 0) : 0;
      const auto = text.getautowidth?.() ?? 0;
      return auto > 0 ? auto + around : 66;
    };
    let barH = this._barH;
    for (const tab of this._tabs) barH = Math.max(barH, groupH(tab.on), groupH(tab.off));
    const spacing = -4;
    let x = 0;
    this._tabs.forEach((tab, i) => {
      const button = i === this._current ? tab.on : tab.off;
      const bw = tabW(button ?? tab.on ?? tab.off);
      for (const b of [tab.on, tab.off]) {
        if (!b) continue;
        b.setxmlparam("relatw", "0");
        b.setxmlparam("relath", "0");
        b.setxmlparam("x", String(x));
        b.setxmlparam("y", "0");
        b.setxmlparam("w", String(bw));
        b.setxmlparam("h", String(barH));
        const div = b.getDiv?.();
        if (div) div.style.zIndex = i === this._current ? "3" : "2";
      }
      x += bw + spacing;
    });
    if (this._tabs.length) x -= spacing;
    if (this._bottom) {
      this._bottom.setxmlparam("x", String(x));
      this._bottom.setxmlparam("y", "0");
      this._bottom.setxmlparam("w", String(Math.max(0, w - x)));
      this._bottom.setxmlparam("h", String(barH));
    }
    if (this._content) {
      this._content.setxmlparam("relatw", "0");
      this._content.setxmlparam("relath", "0");
      this._content.setxmlparam("x", "0");
      this._content.setxmlparam("y", String(barH));
      this._content.setxmlparam("w", String(w));
      this._content.setxmlparam("h", String(Math.max(0, h - barH)));
    }
    // the page where the content's "content" part is
    const part = this._content?.findobject?.("content");
    const px = part ? Number(part._x) || 0 : 0;
    const py = part ? Number(part._y) || 0 : 0;
    const pw = part && part._relatw == "1" ? Number(part._w) || 0 : 0;
    const ph = part && part._relath == "1" ? Number(part._h) || 0 : 0;
    for (const tab of this._tabs) {
      tab.page.setxmlparam("relatw", "0");
      tab.page.setxmlparam("relath", "0");
      tab.page.setxmlparam("x", String(px));
      tab.page.setxmlparam("y", String(barH + py));
      tab.page.setxmlparam("w", String(Math.max(0, w + pw)));
      tab.page.setxmlparam("h", String(Math.max(0, h - barH + ph)));
    }
  }
}
