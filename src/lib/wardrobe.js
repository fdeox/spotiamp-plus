// Lala's wardrobe: outfits she earns with listening badges. The art is drawn in
// Lala/tools/wardrobe.py and exported as one layer per item per animation
// ($lib/mascot/wardrobe), drawn over her by the mascot window. What she wears
// is kept in localStorage, which every window shares: the Listening stats
// window's WARDROBE tab sets it, and the mascot window hears the change.

export const OUTFIT_KEY = "lala-outfit";
/** asks the stats window to open on this tab next time it's shown */
export const STATS_TAB_KEY = "stats-open-tab";

/** the order layers are drawn in, under to over */
export const SLOTS = ["body", "feet", "back", "neck", "head", "face"];

/**
 * Easiest to earn first. `badge` is the badge's name in badges.js.
 * @type {{id: string, name: string, slot: string, badge: string}[]}
 */
export const ITEMS = [
  { id: "sneakers", name: "Sneakers", slot: "feet", badge: "First spin" },
  { id: "cap", name: "Backwards cap", slot: "head", badge: "Warming up" },
  { id: "bow", name: "Pink bow", slot: "head", badge: "Mixtape" },
  { id: "cat_ears", name: "Cat ears", slot: "head", badge: "First love" },
  { id: "shades", name: "Shades", slot: "face", badge: "Ten hours" },
  { id: "bucket_hat", name: "Bucket hat", slot: "head", badge: "Curious" },
  { id: "bandana", name: "Bandana", slot: "neck", badge: "On a roll" },
  { id: "heart_shades", name: "Heart glasses", slot: "face", badge: "Collector" },
  { id: "hoodie", name: "Hoodie", slot: "body", badge: "Hundred" },
  { id: "sweatband", name: "Sweatband", slot: "head", badge: "Marathon" },
  { id: "star_glasses", name: "Star glasses", slot: "face", badge: "Night owl" },
  { id: "jersey", name: "Jersey", slot: "body", badge: "Every day" },
  { id: "backpack", name: "Backpack", slot: "back", badge: "Explorer" },
  { id: "guitar", name: "Guitar", slot: "back", badge: "Fifty" },
  { id: "puffer", name: "Puffer jacket", slot: "body", badge: "Fortnight" },
  { id: "chain", name: "Gold chain", slot: "neck", badge: "Thousand" },
  { id: "leather", name: "Leather jacket", slot: "body", badge: "Century" },
  { id: "halo", name: "Halo", slot: "head", badge: "Habit" },
  { id: "tuxedo", name: "Tuxedo", slot: "body", badge: "Devoted" },
  { id: "crown", name: "Crown", slot: "head", badge: "Llama legend" },
];

const BY_ID = new Map(ITEMS.map((item) => [item.id, item]));

/** What she wears: item ids, one per slot at most. */
export function readOutfit() {
  try {
    const ids = JSON.parse(localStorage.getItem(OUTFIT_KEY) || "[]");
    return Array.isArray(ids) ? ids.filter((id) => BY_ID.has(id)) : [];
  } catch {
    return [];
  }
}

/** @param {string[]} ids */
function saveOutfit(ids) {
  try {
    localStorage.setItem(OUTFIT_KEY, JSON.stringify(ids));
  } catch {}
}

/** Puts an item on (whatever was in its slot comes off), or takes it off. */
export function toggleItem(/** @type {string} */ id) {
  const item = BY_ID.get(id);
  if (!item) return readOutfit();
  const now = readOutfit();
  const next = now.includes(id) ? now.filter((x) => x !== id) : [...now.filter((x) => BY_ID.get(x)?.slot !== item.slot), id];
  saveOutfit(next);
  return next;
}

/** Takes everything off. */
export function undress() {
  saveOutfit([]);
  return [];
}

/** The outfit's ids in drawing order (under to over). */
export function layered(/** @type {string[]} */ ids) {
  return [...ids].sort((a, b) => SLOTS.indexOf(BY_ID.get(a)?.slot ?? "") - SLOTS.indexOf(BY_ID.get(b)?.slot ?? ""));
}
