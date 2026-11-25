/** Where a dragged bot lands relative to the row it is dropped on. */
export type DropPlace = "before" | "after";

/**
 * Moves `botId` next to `targetId` in a project's full order. Returns the
 * order unchanged when either bot is missing or they are the same bot.
 */
export function moveBot(
  order: readonly string[],
  botId: string,
  targetId: string,
  place: DropPlace,
): readonly string[] {
  if (botId === targetId || !order.includes(botId)) {
    return order;
  }
  const without = order.filter((id) => id !== botId);
  const at = without.indexOf(targetId);
  if (at === -1) {
    return order;
  }
  const index = place === "before" ? at : at + 1;
  return [...without.slice(0, index), botId, ...without.slice(index)];
}

/**
 * Moves a bot one step past its neighbour in the rows the user can see.
 *
 * Pinned bots sit in their own strip, so "up" means past the previous plain
 * row, not past a pinned bot that happens to sit between them in the full
 * order. Returns null when there is no neighbour in that direction.
 */
export function stepBot(
  order: readonly string[],
  visible: readonly string[],
  botId: string,
  delta: -1 | 1,
): readonly string[] | null {
  const at = visible.indexOf(botId);
  if (at === -1) {
    return null;
  }
  const neighbour = visible[at + delta];
  if (neighbour === undefined) {
    return null;
  }
  return moveBot(order, botId, neighbour, delta < 0 ? "before" : "after");
}

export function sameOrder(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((id, index) => id === b[index]);
}
