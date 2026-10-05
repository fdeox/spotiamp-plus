<script>
  // Winamp's small time display (the playlist's corner, the main window's
  // shade): TEXT.BMP digits either side of the colon the skin paints itself,
  // laid out as Winamp does: a minus at 1, the minutes at 7 and 12, the
  // seconds at 20 and 25. A colon of our own would land next to the skin's.
  /**
   * @typedef {Object} Props
   * @property {string} minutes up to two digits; a leading "-" is the minus
   * @property {string} seconds two digits
   * @property {number} x
   * @property {number} y
   */

  /** @type {Props} */
  let { minutes, seconds, x, y } = $props();

  /** TEXT.BMP cell of a digit, the minus or a blank: [row, column] */
  /** @param {string} char */
  const cell = (char) => (char === "-" ? [1, 15] : /\d/.test(char) ? [1, Number(char)] : [0, 30]);

  const chars = $derived.by(() => {
    const mm = minutes.replace("-", "").padStart(2, "0").slice(-2);
    const ss = seconds.padStart(2, "0").slice(-2);
    return [
      { at: 1, char: minutes.startsWith("-") ? "-" : " " },
      { at: 7, char: mm[0] },
      { at: 12, char: mm[1] },
      { at: 20, char: ss[0] },
      { at: 25, char: ss[1] },
    ].map(({ at, char }) => ({ at, cell: cell(char) }));
  });
</script>

<div class="mini-time" style:--x={x} style:--y={y}>
  {#each chars as { at, cell }}
    <div
      class="sprite mini-digit"
      style:--at={at}
      style:--row={cell[0]}
      style:--col={cell[1]}
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
    width: 5px;
    height: 6px;
    background-position: calc(-5px * var(--col)) calc(-6px * var(--row));
  }
</style>
