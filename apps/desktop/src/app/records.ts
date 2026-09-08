/** Returns a copy of `source` without `key`, or `source` itself when absent. */
export function withoutKey<T>(
  source: Readonly<Record<string, T>>,
  key: string,
): Readonly<Record<string, T>> {
  if (!(key in source)) {
    return source;
  }
  const next = { ...source };
  delete next[key];
  return next;
}
