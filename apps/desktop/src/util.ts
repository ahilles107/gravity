/**
 * Case-insensitive subsequence match for palette filtering.
 * Returns a score (higher = tighter match) or null when `query` is not a
 * subsequence of `text`. Consecutive and prefix hits score higher.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) {
    return 0;
  }
  const haystack = text.toLowerCase();
  let score = 0;
  let previous = -2;
  let from = 0;
  for (const ch of needle) {
    const index = haystack.indexOf(ch, from);
    if (index === -1) {
      return null;
    }
    score += index === previous + 1 ? 3 : 1;
    if (index === 0) {
      score += 2;
    }
    previous = index;
    from = index + 1;
  }
  return score;
}

export function errText(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function isToday(date: Date): boolean {
  const now = new Date();
  return (
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  );
}

/** Full timestamp for message rows: time today, date + time otherwise. */
export function fmtTimestamp(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  const time = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (isToday(date)) {
    return time;
  }
  const day = date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  return `${day} ${time}`;
}

/** Compact "next run" style timestamp for the sidebar. */
export function fmtShortTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  const time = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (isToday(date)) {
    return time;
  }
  const day = date.toLocaleDateString(undefined, { weekday: "short" });
  return `${day} ${time}`;
}
