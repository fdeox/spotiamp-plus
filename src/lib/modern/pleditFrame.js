// Winamp 5's playlist editor in a modern skin: the skin's own window for it
// (a container whose component is the playlist), or, for a skin without
// one, a window Winamp made itself (gen_ff: skinwnd_createByGuid with its
// "resizable_status" container): the skin's standard frame with a status
// bar, titled "Playlist Editor", 320 x 200, the playlist in the frame's
// room, the playlist's buttons on the status bar. Read for its behaviour;
// none of Winamp's code is used.

export const PLAYLIST_GUID = "{45F3F7C1-A6F3-4ee6-A15E-125E92FC3F8D}";

/** The skin's window holding the playlist, if it has one. @param {any} ui */
export function playlistContainer(ui) {
  return ui.getContainers().find((/** @type {any} */ c) => c.hasId("guid:pl")) ?? null;
}

/** The frame's room for its contents: its script's param (x, y, w, h and
 *  which count from the far side), as Winamp gave the frame scripts.
 * @param {any} groupdef @returns {string | null} the attributes to place it with */
function roomOf(groupdef) {
  /** @type {any[]} */
  const stack = [groupdef];
  while (stack.length) {
    const node = stack.pop();
    if (!node) continue;
    const param = node.name?.toLowerCase() === "script" ? String(node.attributes?.param ?? "") : "";
    if (/^\s*-?\d+\s*(,\s*-?\d+\s*){3}/.test(param)) {
      const [x, y, w, h, rx = 0, ry = 0, rw = 0, rh = 0] = param.split(",").map((n) => Number(n) || 0);
      return `x="${x}" y="${y}" w="${w}" h="${h}" relatx="${rx ? 1 : 0}" relaty="${ry ? 1 : 0}" relatw="${rw ? 1 : 0}" relath="${rh ? 1 : 0}"`;
    }
    for (const child of node.children ?? []) stack.push(child);
  }
  return null;
}

/**
 * The playlist's window: the skin's, or Winamp's own in the skin's frame,
 * made now and kept as one of the skin's windows (hidden until shown).
 * @param {any} ui @param {any} engine
 */
export async function ensurePlaylistWindow(ui, engine) {
  const have = playlistContainer(ui);
  if (have) return have;
  const frame = ["wasabi.standardframe.statusbar", "wasabi.standardframe.nostatusbar"].find((id) => ui.getGroupDef(id));
  const room = (frame && roomOf(ui.getGroupDef(frame))) || `x="11" y="20" w="-22" h="-33" relatw="1" relath="1"`;
  const xml =
    `<container id="spotiamp.pledit" name="Playlist Editor" component="guid:${PLAYLIST_GUID}" default_visible="0" dynamic="1">` +
    `<layout id="normal" w="320" h="200" minimum_w="200" minimum_h="100">` +
    (frame ? `<group id="${frame}" x="0" y="0" w="0" h="0" relatw="1" relath="1"/>` : "") +
    `<windowholder hold="guid:${PLAYLIST_GUID}" ${room}/>` +
    `</layout></container>`;
  const node = engine.parseXmlFragment(xml).children[0].children.find((/** @type {any} */ n) => n.name === "container");
  const container = await engine.container(node);
  container.draw();
  ui.getRootDiv().appendChild(container.getDiv());
  container.init();
  return container;
}
