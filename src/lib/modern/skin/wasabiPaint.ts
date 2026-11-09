// @ts-nocheck: works on the engine's objects, which are untyped
// Spotiamp+: pieces of Wasabi's own drawing that the skin's pictures are
// drawn with: a skin bitmap in the colour theme on, and a Wasabi button
// (Paintset's BUTTONUP / BUTTONDOWN: the wasabi.button.* corners, sides and
// middle stretched to the rect), as Winamp drew its playlist's scrollbar
// button and its status bar's buttons. Read for its behaviour; none of
// Winamp's code is used.

/** A skin bitmap in the colour theme on, as a canvas (null if it has none). */
export function bitmapCanvas(ui, id: string): HTMLCanvasElement | null {
  const lower = String(id ?? "").toLowerCase();
  if (!lower || (!ui.hasBitmap(lower) && !ui._elementAlias?.[lower])) return null;
  let bitmap;
  try {
    bitmap = ui.getBitmap(id);
  } catch {
    return null;
  }
  const img = bitmap?.getImg?.();
  if (!img) return null;
  try {
    const group = ui._getGammaGroup(bitmap.getGammaGroup());
    if (group.isIdentity()) return bitmap.getCanvas();
    return group.transformCanvas(img, bitmap.getLeft(), bitmap.getTop(), bitmap.getWidth() || img.width, bitmap.getHeight() || img.height);
  } catch {
    return null;
  }
}

/** A Wasabi button (or its pressed look) stretched to the rect, with a
 *  picture in its middle when given one (gen_ff's blitButtonToCanvas: the
 *  scrollbar's grip, a pixel lower and righter when pressed). */
export function drawWasabiButton(ui, ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, pushed: boolean, overlay: string = "") {
  const pre = pushed ? "wasabi.button.pressed." : "wasabi.button.";
  const [ul, u, ur, l, mid, r, ll, bot, lr] = ["top.left", "top", "top.right", "left", "middle", "right", "bottom.left", "bottom", "bottom.right"].map((n) =>
    bitmapCanvas(ui, pre + n)
  );
  const W = (c) => (c ? c.width : 0);
  const H = (c) => (c ? c.height : 0);
  const put = (c, left: number, top: number, right: number, bottom: number) => {
    if (c && c.width && c.height && right > left && bottom > top) ctx.drawImage(c, 0, 0, c.width, c.height, left, top, right - left, bottom - top);
  };
  const smooth = ctx.imageSmoothingEnabled;
  ctx.imageSmoothingEnabled = false;
  put(ul, x, y, x + W(ul), y + H(ul));
  put(u, x + W(ul), y, x + w - W(ur), y + H(ul));
  put(ur, x + w - W(ur), y, x + w, y + H(ul));
  put(l, x, y + H(ul), x + W(l), y + h - H(ll));
  put(mid, x + W(ul), y + H(ul), x + w - W(ur), y + h - H(ll));
  put(r, x + w - W(r), y + H(ur), x + w, y + h - H(lr));
  put(ll, x, y + h - H(ll), x + W(ll), y + h);
  put(bot, x + W(ll), y + h - H(bot), x + w - W(lr), y + h);
  put(lr, x + w - W(lr), y + h - H(lr), x + w, y + h);
  if (overlay) {
    const pic = bitmapCanvas(ui, overlay);
    if (pic && pic.width && pic.height) {
      const shift = pushed ? 1 : 0;
      ctx.drawImage(pic, x + Math.trunc((w - pic.width) / 2) + shift, y + Math.trunc((h - pic.height) / 2) + shift);
    }
  }
  ctx.imageSmoothingEnabled = smooth;
}
