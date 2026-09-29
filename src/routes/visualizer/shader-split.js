// Both visualizer shaders pick their pattern with one long chain in main():
//   if (m == 0) { ... } else if (m == 1) { ... } ... else { ... }
// Compiled whole, that's slow: ANGLE's D3D compiler gets much slower as a
// shader grows (the first fifty patterns took ~2.5 s, a hundred would take
// far longer), and the window froze meanwhile. So each pattern gets its own
// small program instead: the same text with every other branch left out.
// Everything outside the chain (helpers, post-processing, trails) is kept as
// is, so a pattern renders exactly as it did inside the full shader.

const CHAIN_START = "      if (m == 0) {\n";
const BRANCH = /\n {6}\} else if \(m == (\d+)\) \{\n|\n {6}\} else \{\n/g;
const CHAIN_END = "\n      }\n";

/**
 * @param {string} frag the whole shader
 * @param {number} count how many patterns the chain should hold
 * @returns {(i: number) => string} the source for pattern i; the whole shader
 *   if the chain isn't laid out as expected (slower, but still correct)
 */
export function patternSources(frag, count) {
  const whole = () => frag;
  // (Template literals already turn CRLF into LF; this is for any other source.)
  frag = frag.replace(/\r\n/g, "\n");
  const first = frag.indexOf(CHAIN_START);
  if (first < 0) return whole;
  const cuts = [];
  BRANCH.lastIndex = first;
  let match;
  while ((match = BRANCH.exec(frag))) {
    // An `else if` must carry the next number in order, and the last cut must
    // be the plain `else`; anything else means the layout isn't what's expected.
    if (match[1] !== undefined && Number(match[1]) !== cuts.length + 1) return whole;
    cuts.push({ at: match.index, len: match[0].length, last: match[1] === undefined });
    if (match[1] === undefined) break;
  }
  if (cuts.length !== count - 1 || !cuts[cuts.length - 1].last) return whole;
  const end = frag.indexOf(CHAIN_END, cuts[cuts.length - 1].at + 1);
  if (end < 0) return whole;
  const head = frag.slice(0, first);
  const tail = frag.slice(end + CHAIN_END.length);
  /** @type {string[]} */
  const bodies = [];
  let from = first + CHAIN_START.length;
  for (const cut of cuts) {
    bodies.push(frag.slice(from, cut.at));
    from = cut.at + cut.len;
  }
  bodies.push(frag.slice(from, end));
  return (i) => `${head}      if (m == ${i}) {\n${bodies[i]}\n      }\n${tail}`;
}
