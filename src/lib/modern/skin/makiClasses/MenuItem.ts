// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { installGlobalMouseDown, uninstallGlobalMouseDown } from "./GuiObj";

export interface IPopupMenu {
  children: MenuItem[];
}

export type IMenuItem = {
  type: "menuitem";
  caption: string;
  id: number;
  checked: boolean;
  disabled?: boolean;
  shortcut?: string; // "Ctrl+Alt+Shift+A"
  keychar?: string; // 'p' of "&Play"
  invisible?: boolean; // special case to register shortcut only
  data?: { [key: string]: any }; // used by skin's popup item
};

type IMenuSeparator = {
  type: "separator";
};

type IMenuPopup = {
  type: "popup";
  caption: string;
  popup: IPopupMenu;
  checked?: boolean;
  disabled?: boolean;
  children?: MenuItem[];
};

export type MenuItem = IMenuItem | IMenuSeparator | IMenuPopup;

export interface ICLoseablePopup {
  doClosePopup: Function;
}

// ################## Popup Utils ##########################33

let ACTIVE_POPUP: ICLoseablePopup = null;

export function destroyActivePopup() {
  console.log("globalWindowClick");
  if (ACTIVE_POPUP != null) {
    ACTIVE_POPUP.doClosePopup();
  }
  // ACTIVE_MENU_GROUP = ''
  uninstallGlobalClickListener();
}

export function setActivePopup(popup: ICLoseablePopup) {
  ACTIVE_POPUP = popup;
  if (popup) {
    installGlobalClickListener();
  } else {
    uninstallGlobalClickListener();
  }
}

/**
 * if the active == popup => set null
 * @param popup
 */
export function deactivePopup(popup: ICLoseablePopup) {
  if (popup == ACTIVE_POPUP) {
    setActivePopup(null);
  }
}

let globalClickInstalled = false;
function installGlobalClickListener() {
  setTimeout(() => {
    // using promise to prevent immediately executing of globalWindowClick
    if (!globalClickInstalled) {
      installGlobalMouseDown(destroyActivePopup); // call globalWindowClick on any GuiObj
      document.addEventListener("mousedown", destroyActivePopup); // call globalWindowClick on document
      globalClickInstalled = true;
    }
  }, 500);
}

function uninstallGlobalClickListener() {
  if (globalClickInstalled) {
    document.removeEventListener("mousedown", destroyActivePopup);
    globalClickInstalled = false;
    uninstallGlobalMouseDown(destroyActivePopup);
  }
}

// ################## MenuItem Utils ##########################33

export function forEachMenuItem(popup: IPopupMenu, callback: Function) {
  for (const menu of popup.children) {
    if (menu.type == "menuitem") {
      callback(menu);
    } else if (menu.type == "popup") {
      forEachMenuItem(menu.popup, callback);
    }
  }
}

type ExtractedCaptions = {
  caption: string;
  shortcut?: string; // "Ctrl+Alt+Shift+A"
  keychar?: string; // 'p' of "&Play"
};

export function extractCaption(text: string): ExtractedCaptions {
  // const result:ExtractedCaptions = {caption:text, shortcut:null, keychar:''}
  const [caption, shortcut] = text.split("\t");
  const keychar = caption.includes("&")
    ? caption[caption.indexOf("&") + 1].toLowerCase()
    : "";
  return { caption, shortcut, keychar };
}

export function generatePopupDiv(
  popup: IPopupMenu,
  callback: Function
): HTMLElement {
  const root = document.createElement("ul");
  root.className = "popup-menu-container";
  // root.style.zIndex = "1000";
  // console.log('generating popup:', popup)
  for (const menu of popup.children) {
    // const menuitem = document.createElement("li");
    let item: HTMLElement;
    // root.appendChild(item);
    switch (menu.type) {
      case "menuitem":
        if (menu.invisible === true) {
          continue;
        }
        item = generatePopupItem(menu);
        item.addEventListener("mousedown", (e) => callback(menu.id));
        // item.onclick = (e) => callback(menu.id);
        break;
      case "popup":
        item = generatePopupItem(menu);
        const subMenu = generatePopupDiv(menu.popup, callback);
        item.appendChild(subMenu);
        break;
      case "separator":
        item = document.createElement("hr");
        break;
    }
    root.appendChild(item);
  }
  return root;
}

//? one row of popup
function generatePopupItem(menu: IMenuItem | IMenuPopup): HTMLElement {
  const item = document.createElement("li");

  //? checkmark
  const checkMark = document.createElement("span");
  checkMark.classList.add("checkmark");
  checkMark.textContent = menu.checked ? "✓" : " ";
  item.appendChild(checkMark);

  //? display text
  // @ts-ignore
  // const [caption, keystroke] = menu.caption.split('\t')
  const label = generateCaption(menu.caption);
  label.classList.add("caption");
  item.appendChild(label);

  //? keystroke

  const shortcut = document.createElement("span");
  shortcut.classList.add("keystroke");
  shortcut.textContent = menu.type == "menuitem" ? menu.shortcut : "";
  item.appendChild(shortcut);

  //? sub-menu sign
  const chevron = document.createElement("span");
  chevron.classList.add("chevron");
  chevron.textContent = menu.type == "popup" ? "🞂" : " ";
  item.appendChild(chevron);
  // item.textContent = `${menu.checked? '✓' : ' '} ${menu.caption}`;

  return item;
}

function generateCaption(caption: string): HTMLElement {
  // Spotiamp+: built as text (a skin's captions are the skin maker's, not
  // HTML to run); "&x" underlines the x, as in Windows' menus
  const span = document.createElement("span");
  span.classList.add("caption");
  const parts = caption.split(/&(\w)/);
  parts.forEach((part, i) => {
    if (i % 2 === 1) {
      const u = document.createElement("u");
      u.textContent = part;
      span.appendChild(u);
    } else if (part) {
      span.appendChild(document.createTextNode(part));
    }
  });
  return span;
}
