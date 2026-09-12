// Pure shaping for the Control center: ordering, relative time, deadlines and
// the registry filter. Kept out of the components so the rules that matter —
// what sorts first, what reads as urgent — are testable directly.

import type { Decision } from "../../protocol/decisions";

const HOUR_MS = 60 * 60 * 1000;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTHS_LONG = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/**
 * Urgent first, then what runs out first, then what has waited longest.
 *
 * A decision with no deadline sorts after every dated one: a bot that named a
 * date is telling you the choice stops being available.
 */
function byUrgencyDeadlineAge(a: Decision, b: Decision): number {
  const urgency = Number(b.priority === "urgent") - Number(a.priority === "urgent");
  if (urgency !== 0) {
    return urgency;
  }
  const left = a.deadline_at ?? "9999";
  const right = b.deadline_at ?? "9999";
  if (left !== right) {
    return left < right ? -1 : 1;
  }
  if (a.created_at !== b.created_at) {
    return a.created_at < b.created_at ? -1 : 1;
  }
  return a.id < b.id ? -1 : 1;
}

/** A fresh list in the order the waiting list shows. */
export function sortWaiting(decisions: readonly Decision[]): readonly Decision[] {
  const copy = [...decisions];
  // oxlint-disable-next-line unicorn/no-array-sort
  copy.sort(byUrgencyDeadlineAge);
  return copy;
}

/** The moment a registry row is filed under: when it was ruled, else when it changed. */
export function filedAt(decision: Decision): string {
  return decision.published_at ?? decision.edited_at ?? decision.created_at;
}

/** Newest ruling first, for the ledger. */
export function sortByFiledAt(decisions: readonly Decision[]): readonly Decision[] {
  const copy = [...decisions];
  // oxlint-disable-next-line unicorn/no-array-sort
  copy.sort((a, b) => (filedAt(a) < filedAt(b) ? 1 : filedAt(a) > filedAt(b) ? -1 : 0));
  return copy;
}

/**
 * Bot names the ask travelled through, for the via-chain. The chain is stored
 * as bot ids because names change; it is resolved here so the owner sees who
 * is actually waiting.
 */
export function viaChain(
  decision: Decision,
  nameOf: (botId: string) => string | undefined,
): readonly string[] {
  return decision.origin_chain
    .split(",")
    .map((id) => id.trim())
    .filter((id) => id.length > 0 && id !== decision.raised_by.bot_id)
    .map((id) => nameOf(id) ?? "a deleted bot");
}

/**
 * A ruling a bot relayed rather than the owner typed. Shown as such until it
 * is confirmed: the whole point of the registry is that authority is legible.
 */
export function isRelayed(decision: Decision): boolean {
  return decision.ruling?.answered_by.startsWith("owner-via-bot:") ?? false;
}

/** "just now", "3h ago", "2 days ago" — enough to see what has been waiting. */
export function age(iso: string, now: number): string {
  const then = Date.parse(iso);
  if (!Number.isFinite(then)) {
    return "";
  }
  const hours = Math.max(0, (now - then) / HOUR_MS);
  if (hours < 1) {
    return "just now";
  }
  if (hours < 24) {
    return `${Math.round(hours)}h ago`;
  }
  const days = Math.floor(hours / 24);
  return `${days} ${days === 1 ? "day" : "days"} ago`;
}

/** Hours a settled decision stayed open, never less than one. */
export function hoursOpen(decision: Decision): number | undefined {
  if (decision.published_at === undefined) {
    return undefined;
  }
  const ms = Date.parse(decision.published_at) - Date.parse(decision.created_at);
  return Number.isFinite(ms) ? Math.max(1, Math.round(ms / HOUR_MS)) : undefined;
}

/** "12 Sep" */
export function fmtDay(iso: string): string {
  const d = new Date(iso);
  return `${d.getDate()} ${MONTHS[d.getMonth()] ?? ""}`;
}

/** "14:00" */
export function fmtTime(iso: string): string {
  const d = new Date(iso);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "September 2026" */
function fmtMonth(iso: string): string {
  const d = new Date(iso);
  return `${MONTHS_LONG[d.getMonth()] ?? ""} ${d.getFullYear()}`;
}

type DeadlineTone = "dim" | "amber" | "red";

export interface DeadlineLabel {
  /** What the list shows: "6h left", "tomorrow", "until 13 Sep", "closed". */
  readonly short: string;
  readonly tone: DeadlineTone;
  /** The same fact as a sentence, for the reading pane. Absent once closed. */
  readonly long?: string;
}

/** Grey until the deadline is inside 24 hours, then amber; red once it passed. */
export function deadline(iso: string | undefined, now: number): DeadlineLabel | undefined {
  if (iso === undefined) {
    return undefined;
  }
  const due = Date.parse(iso);
  if (!Number.isFinite(due)) {
    return undefined;
  }
  const hours = (due - now) / HOUR_MS;
  if (hours < 0) {
    return { short: "closed", tone: "red" };
  }
  if (hours < 24) {
    const h = Math.max(1, Math.round(hours));
    const day = new Date(due).getDate() === new Date(now).getDate() ? "today" : "tomorrow";
    return {
      short: `${h}h left`,
      tone: "amber",
      long: `Choice closes in ${h} ${h === 1 ? "hour" : "hours"}, ${fmtTime(iso)} ${day}`,
    };
  }
  if (hours < 48) {
    return {
      short: "tomorrow",
      tone: "amber",
      long: `Choice closes tomorrow, ${fmtDay(iso)} ${fmtTime(iso)}`,
    };
  }
  return {
    short: `until ${fmtDay(iso)}`,
    tone: "dim",
    long: `Choice stops being available after ${fmtDay(iso)}`,
  };
}

/** What the ledger is narrowed to: the search box and the three chip rows. */
export interface RegistryFilter {
  readonly query: string;
  readonly tagFilter: string | undefined;
  readonly projectFilter: string | undefined;
  readonly botFilter: string | undefined;
}

export const NO_FILTER: RegistryFilter = {
  query: "",
  tagFilter: undefined,
  projectFilter: undefined,
  botFilter: undefined,
};

/**
 * The registry search: title, the question itself, the owner's words, who
 * asked, and the tags, narrowed by the tag, project and bot chips.
 *
 * The body is in here because it is what the owner actually remembers — the
 * daemon indexes it too, and leaving it out meant searching for a phrase from
 * a decision you had read returned nothing.
 */
export function matchesRegistry(decision: Decision, filter: RegistryFilter): boolean {
  if (filter.tagFilter !== undefined && !decision.tags.includes(filter.tagFilter)) {
    return false;
  }
  if (filter.projectFilter !== undefined && decision.project_id !== filter.projectFilter) {
    return false;
  }
  if (filter.botFilter !== undefined && decision.raised_by.bot_id !== filter.botFilter) {
    return false;
  }
  const needle = filter.query.trim().toLowerCase();
  if (needle === "") {
    return true;
  }
  const haystack = [
    decision.title,
    decision.body,
    decision.ruling?.text ?? "",
    decision.ruling?.reason ?? "",
    decision.withdrawn_reason ?? "",
    decision.raised_by.name,
    decision.tags.join(" "),
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(needle);
}

/** One project or bot chip: what it filters to, what it reads as, how many rows it holds. */
export interface Facet {
  readonly id: string;
  readonly label: string;
  readonly count: number;
}

export interface RegistryFacets {
  readonly projects: readonly Facet[];
  readonly bots: readonly Facet[];
}

function tally(rows: readonly Decision[], key: (row: Decision) => Facet): readonly Facet[] {
  const counts = new Map<string, Facet>();
  for (const row of rows) {
    const facet = key(row);
    const seen = counts.get(facet.id);
    counts.set(facet.id, { ...facet, count: (seen?.count ?? 0) + 1 });
  }
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...counts.values()].sort((a, b) => a.label.localeCompare(b.label));
}

/**
 * The project and bot chips the ledger offers, with their counts.
 *
 * Each row is counted against the filters that sit above it, so picking a
 * project renarrows the bot chips to the bots that raised something in it —
 * chips for bots with nothing left to show are worse than no chips at all.
 */
export function registryFacets(
  rows: readonly Decision[],
  filter: RegistryFilter,
  projectName: (projectId: string) => string,
): RegistryFacets {
  const bare = { ...filter, projectFilter: undefined, botFilter: undefined };
  const searched = rows.filter((row) => matchesRegistry(row, bare));
  const inProject =
    filter.projectFilter === undefined
      ? searched
      : searched.filter((row) => row.project_id === filter.projectFilter);
  return {
    projects: tally(searched, (row) => ({
      id: row.project_id,
      label: projectName(row.project_id),
      count: 0,
    })),
    bots: tally(inProject, (row) => ({
      id: row.raised_by.bot_id,
      label: row.raised_by.name,
      count: 0,
    })),
  };
}

export interface MonthGroup {
  readonly label: string;
  readonly decisions: readonly Decision[];
}

/**
 * Rows bucketed by the month they were filed, in the order given.
 *
 * Appends in place rather than respreading the accumulated bucket: a month
 * holding n rulings cost n²/2 array copies, on every render of a list that
 * re-renders on each keystroke and each clock tick.
 */
export function groupByMonth(decisions: readonly Decision[]): readonly MonthGroup[] {
  const groups: { label: string; decisions: Decision[] }[] = [];
  for (const decision of decisions) {
    const label = fmtMonth(filedAt(decision));
    const last = groups.at(-1);
    if (last !== undefined && last.label === label) {
      last.decisions.push(decision);
    } else {
      groups.push({ label, decisions: [decision] });
    }
  }
  return groups;
}

/** A `<input type="date">` value as the RFC 3339 instant that day begins, locally. */
export function dayToIso(day: string): string | undefined {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (match === null) {
    return undefined;
  }
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return Number.isFinite(date.getTime()) ? date.toISOString() : undefined;
}
