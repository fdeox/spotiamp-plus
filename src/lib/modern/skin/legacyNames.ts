// Spotiamp+: Winamp 3's names for Wasabi's elements, as Winamp 5 still took
// them. A skin made for Winamp 3 (or early Winamp 5) names its list colours,
// buttons, scrollbars and menus "studio.*" ("studio.list.text",
// "studio.button.upperLeft"); Winamp 5 asks for "wasabi.*" ones, and a skin's
// element of an old name was taken as the new one when the skin was read
// (skinelem.cpp: a bitmap's or colour's id, if it's an alias, is replaced by
// its target). These rules give the new name of an old one; null when the
// name isn't one of them.

const CORNERS: Record<string, string> = {
  upperleft: "top.left",
  upperright: "top.right",
  lowerleft: "bottom.left",
  lowerright: "bottom.right",
};

const SAME_REST =
  /^(basetexture|framewnd\..+|list\..+|tree\..+|msgbox\.background|textbar\.(text|background)|button\.(text|hilitetext|dimmedtext)|popup\.(text|hilitetext|dimmedtext)|component\.title\.(foreground|border))$/;

/** The Winamp 5 name of a Winamp 3 element name (lowercase), or null. */
export function legacyTarget(id: string): string | null {
  const name = String(id ?? "").toLowerCase();
  let m: RegExpMatchArray | null;
  // (a misspelling skins of the time used)
  if (name === "sutdio.list.background") return "wasabi.list.background";
  if ((m = name.match(/^common\.(itemlist|labelwnd)\.(.+)$/))) return `wasabi.${m[1]}.${m[2]}`;
  if (!name.startsWith("studio.")) return null;
  const rest = name.slice("studio.".length);
  if ((m = rest.match(/^(button(?:\.pressed)?|label)\.(upperleft|upperright|lowerleft|lowerright)$/))) return `wasabi.${m[1]}.${CORNERS[m[2]]}`;
  if ((m = rest.match(/^(button(?:\.pressed)?|label)\.(top|left|middle|right|bottom)$/))) return `wasabi.${m[1]}.${m[2]}`;
  if ((m = rest.match(/^scrollbar\.(horizontal|vertical)\.(left|right|button)(\.hilite|\.pressed)?$/))) {
    return `wasabi.scrollbar.${m[1]}.${m[2]}${m[3] === ".hilite" ? ".hover" : (m[3] ?? "")}`;
  }
  if (rest === "button.checkmark") return "wasabi.popup.menu.check";
  if (rest === "popup.submenuicon") return "wasabi.popup.menu.submenu";
  if ((m = rest.match(/^popup\.(seperator|background|border\..+|selection\..+)$/))) return `wasabi.popup.menu.${m[1]}`;
  if ((m = rest.match(/^seekbar\.(left|middle|right)$/))) return `wasabi.slider.horizontal.${m[1]}`;
  if ((m = rest.match(/^seekbar\.(top|bottom)$/))) return `wasabi.slider.vertical.${m[1]}`;
  if (rest === "seekbar.button.nonpressed") return "wasabi.slider.horizontal.button";
  if (rest === "seekbar.button.pressed") return "wasabi.slider.horizontal.button.pressed";
  if (SAME_REST.test(rest)) return `wasabi.${rest}`;
  return null;
}
