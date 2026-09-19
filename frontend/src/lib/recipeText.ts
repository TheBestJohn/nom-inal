/**
 * Turn free-text instructions into steps.
 *
 * Instructions are stored as one string because that is what people type:
 * a line per step, usually, sometimes with their own "1." in front, sometimes
 * as a single paragraph with the numbers inline. Rendering that string as-is
 * in a box is what made the recipe page read like a form. This works out the
 * steps so the page can number them, and leaves a real single paragraph alone
 * rather than presenting it as a one-item list.
 */

/** "1.", "1)", "Step 1:", "- ", "• " — the markers people write themselves. */
const LEADING_MARKER = /^(?:(?:step\s*)?\d{1,3}\s*[.):\-–]|[-*•])\s*/i;

/** A number-dot-space in the middle of a paragraph: "…then 2. Add the…". */
const INLINE_MARKER = /\s+(?=\d{1,3}[.)]\s)/;

export function instructionSteps(text: string): string[] {
  let lines = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);

  // One long line reading "1. … 2. … 3. …" is a list that lost its line
  // breaks, not a paragraph. It has to open with a number to count: a real
  // paragraph that happens to contain "2. " partway through is left alone.
  if (lines.length === 1 && /^\d{1,3}[.)]\s/.test(lines[0])) {
    lines = lines[0].split(INLINE_MARKER);
  }

  return lines.map((l) => l.replace(LEADING_MARKER, "").trim()).filter(Boolean);
}
