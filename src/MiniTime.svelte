<script>
  // Winamp's small time display (the playlist's corner, the main window's
  // shade): TEXT.BMP digits either side of the colon the skin paints itself,
  // laid out as Winamp does (draw_pe_timedisp, draw_main's shade): the
  // minutes at 7 and 12, the seconds at 20 and 25, the minus (3 px of its
  // letter) at 2 in the playlist, 3 in the shade, with blanks left of it.
  // Cleared (nothing playing) every place is a blank. A colon of our own
  // would land next to the skin's.
  /**
   * @typedef {Object} Props
   * @property {string} minutes up to two digits; a leading "-" is the minus
   * @property {string} seconds two digits
   * @property {number} x
   * @property {number} y
   * @property {"playlist" | "shade"} [layout] which of Winamp's two it is
   * @property {boolean} [clear] blanks only (nothing playing)
   */

  /** @type {Props} */
  let { minutes, seconds, x, y, layout = "playlist", clear = false } = $props();

  // TEXT.BMP pixels: a digit's letter, the minus, and the blank Winamp
  // copies (5 px from x=142, across two of the font's spaces)
  /** @param {string} d */
  const digit = (d) => ({ sx: 5 * Number(d), sy: 6 });
  const MINUS = { sx: 75, sy: 6 };
  const BLANK = { sx: 142, sy: 0 };

  const parts = $derived.by(() => {
    // the minus's place, and the blank left of it
    const [minusAt, leftAt, leftW] = layout === "shade" ? [3, -1, 3] : [2, -2, 4];
    const digits = [7, 12, 20, 25];
    if (clear) {
      return [
        { at: leftAt, w: leftW, ...BLANK },
        { at: minusAt, w: layout === "shade" ? 3 : 5, ...BLANK },
        ...digits.map((at) => ({ at, w: 5, ...BLANK })),
      ];
    }
    const negative = minutes.startsWith("-");
    const mm = minutes.replace("-", "").padStart(2, "0").slice(-2);
    const ss = seconds.padStart(2, "0").slice(-2);
    return [
      // (the shade leaves the blank out when the minus is there)
      ...(layout === "shade" && negative ? [] : [{ at: leftAt, w: leftW, ...BLANK }]),
      { at: minusAt, w: 3, ...(negative ? MINUS : BLANK) },
      ...[mm[0], mm[1], ss[0], ss[1]].map((d, i) => ({ at: digits[i], w: 5, ...digit(d) })),
    ];
  });
</script>

<div class="mini-time" style:--x={x} style:--y={y}>
  {#each parts as { at, w, sx, sy }}
    <div
      class="sprite mini-digit"
      style:--at={at}
      style:width="{w}px"
      style:background-position="{-sx}px {-sy}px"
    ></div>
  {/each}
</div>

<style>
  .mini-time {
    position: absolute;
    width: calc(30px * var(--zoom));
    height: calc(6px * var(--zoom));
    left: calc(var(--x) * var(--zoom) * 1px);
    top: calc(var(--y) * var(--zoom) * 1px);
    pointer-events: none;
  }
  .mini-digit {
    --sprite-url: var(--skin-text);
    --sprite-x: calc(var(--at) * 1px);
    --sprite-y: 0px;
    height: 6px;
  }
</style>
