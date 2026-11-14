// The geometry of the grouped, virtualised card grid. Pure maths, no DOM: every card is
// `rowHeight - gap` tall, every section title `headerHeight`, and a section is its title, a gap and
// its card rows, so a section's height is `headerHeight + rows * rowHeight` and offsets are exact.
// The shell sends every group's card count (`groups.layout`); the frontend never learns the
// shell's order or grouping, only these sizes.

export interface GroupMetrics {
  /** Cards per row. */
  cols: number;
  /** Card height plus the gap: the pitch of a row. */
  rowHeight: number;
  headerHeight: number;
  /** The space between sections, between a title and its cards, and between rows. */
  gap: number;
}

export interface GroupOffsets {
  /** Distance of each section's top from the top of the list. */
  top: number[];
  height: number[];
  total: number;
}

const rowsOf = (count: number, cols: number) => Math.ceil(count / cols);

/** Where every section starts, and the height of the whole list. */
export function groupOffsets(layout: readonly number[], m: GroupMetrics): GroupOffsets {
  const top: number[] = [];
  const height: number[] = [];
  let at = 0;
  for (const count of layout) {
    const h = m.headerHeight + rowsOf(count, m.cols) * m.rowHeight;
    top.push(at);
    height.push(h);
    at += h + m.gap;
  }
  return { top, height, total: Math.max(0, at - m.gap) };
}

/**
 * The card index range on screen: from the first card of the first visible row to the last
 * visible one, plus one more row so a scroll never shows a gap. A section whose title is visible
 * but whose cards are not yet counts its first row, so its title is among those the shell sends.
 * A collapsed group has 0 cards in `layout`: it is a title only, 32px tall.
 */
export function visibleGroupedRange(
  scrollTop: number,
  viewHeight: number,
  layout: readonly number[],
  offsets: GroupOffsets,
  m: GroupMetrics,
): { start: number; count: number } {
  const bottom = scrollTop + viewHeight;
  let first = -1;
  let last = -1;
  let before = 0;
  // Where in the card list the first visible section sits: what to ask for when only titles of
  // collapsed groups (no cards) are on screen.
  let anchor = -1;
  for (let i = 0; i < layout.length; i += 1) {
    const count = layout[i] ?? 0;
    const top = offsets.top[i] ?? 0;
    if (top >= bottom) {
      break;
    }
    if (top + (offsets.height[i] ?? 0) > scrollTop && anchor < 0) {
      anchor = before;
    }
    if (count > 0 && top + (offsets.height[i] ?? 0) > scrollTop) {
      const rows = rowsOf(count, m.cols);
      const rowsTop = top + m.headerHeight + m.gap;
      const firstRow = Math.min(rows - 1, Math.max(0, Math.floor((scrollTop - rowsTop) / m.rowHeight)));
      const lastRow = Math.min(rows - 1, Math.max(firstRow, Math.floor((bottom - rowsTop) / m.rowHeight)));
      if (first < 0) {
        first = before + firstRow * m.cols;
      }
      last = before + Math.min(count, (lastRow + 1) * m.cols) - 1;
    }
    before += count;
  }
  if (first < 0) {
    return { start: Math.max(0, anchor), count: m.cols };
  }
  return { start: first, count: Math.min(before - first, last - first + 1 + m.cols) };
}

export interface DrawnGroup {
  /** Index in the whole list of groups. */
  index: number;
  /** The rows drawn: `[rowFrom, rowTo)`, as card offsets inside the group `[cardFrom, cardTo)`. */
  cardFrom: number;
  cardTo: number;
  /** Spacers inside the group's card grid for the rows not drawn. */
  padTop: number;
  padBottom: number;
}

export interface GroupPlan {
  /** Spacers around all drawn groups. */
  padTop: number;
  padBottom: number;
  groups: DrawnGroup[];
}

/**
 * What to draw for the groups the shell sent (`headers`: each group's first card index, in order)
 * and the card slice `[cardsStart, cardsEnd)` it sent: for each group only the whole rows whose
 * cards are present (a partial row only at the end of its group), with spacers for the rest so
 * the sections keep their exact heights. Returns `null` when the headers are not a run of consecutive groups of `layout`.
 */
export function planGroups(
  headers: readonly { index: number; start: number }[],
  cardsStart: number,
  cardsEnd: number,
  layout: readonly number[],
  offsets: GroupOffsets,
  m: GroupMetrics,
): GroupPlan | null {
  const firstHeader = headers[0];
  if (!firstHeader) {
    return { padTop: 0, padBottom: offsets.total, groups: [] };
  }
  const firstIndex = firstHeader.index;
  const groups: DrawnGroup[] = [];
  for (const [n, header] of headers.entries()) {
    const index = firstIndex + n;
    // The shell sends a run of consecutive groups; anything else cannot be placed.
    if (header.index !== index || index >= layout.length) {
      return null;
    }
    const count = layout[index] ?? 0;
    if (count === 0) {
      // Collapsed: a title and nothing under it.
      groups.push({ index, cardFrom: 0, cardTo: 0, padTop: 0, padBottom: 0 });
      continue;
    }
    const present = Math.max(0, Math.min(count, cardsEnd - header.start));
    const from = Math.max(0, cardsStart - header.start);
    const rows = rowsOf(count, m.cols);
    const rowFrom = rowsOf(from, m.cols);
    const rowTo = Math.max(rowFrom, present >= count ? rows : Math.floor(present / m.cols));
    const drawn = rowTo - rowFrom;
    groups.push({
      index,
      cardFrom: rowFrom * m.cols,
      cardTo: Math.min(count, rowTo * m.cols),
      // The grid's own height is rows * pitch - gap. With no row drawn there is no gap to give back,
      // and a spacer of `rowFrom` pitches (rowFrom can equal `rows`) would overshoot by one: one spacer.
      padTop: drawn === 0 ? rows * m.rowHeight - m.gap : rowFrom * m.rowHeight,
      padBottom: drawn === 0 ? 0 : (rows - rowTo) * m.rowHeight,
    });
  }
  const lastIndex = firstIndex + headers.length - 1;
  const top = offsets.top[firstIndex] ?? 0;
  const bottom = (offsets.top[lastIndex] ?? 0) + (offsets.height[lastIndex] ?? 0);
  return { padTop: top, padBottom: Math.max(0, offsets.total - bottom), groups };
}
