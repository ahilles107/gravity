import { Fragment, useMemo } from "react";
import type { ReactElement, RefObject } from "react";
import { useScrollSelectedIntoView } from "../../hooks/useScrollSelectedIntoView";
import type { Decision, Tag } from "../../protocol/decisions";
import BotAvatar from "../BotAvatar";
import { filedAt, fmtDay, groupByMonth, matchesRegistry, registryFacets } from "./decisions";
import type { Facet, RegistryFilter } from "./decisions";

interface RegistryProps {
  /** Settled and withdrawn, newest first. */
  readonly rows: readonly Decision[];
  /** How many rulings exist, so the summary reads "3 of 41". */
  readonly settledCount: number;
  readonly tags: readonly Tag[];
  readonly filter: RegistryFilter;
  readonly onQuery: (query: string) => void;
  readonly onTagFilter: (tag?: string) => void;
  readonly onProjectFilter: (projectId?: string) => void;
  readonly onBotFilter: (botId?: string) => void;
  readonly searchRef: RefObject<HTMLInputElement>;
  /** The keyboard cursor: highlighted, not yet opened. */
  readonly cursorId?: string;
  readonly onOpen: (decisionId: string) => void;
  readonly projectName: (projectId: string) => string;
}

interface LedgerRowProps {
  readonly decision: Decision;
  readonly selected: boolean;
  readonly onOpen: (decisionId: string) => void;
  readonly projectName: (projectId: string) => string;
}

interface ChipRowProps {
  /** What the row narrows, shown ahead of the chips: "tag", "project", "bot". */
  readonly label: string;
  readonly allLabel: string;
  readonly chips: readonly Facet[];
  readonly chosen: string | undefined;
  readonly onChoose: (id?: string) => void;
}

function totalUses(tag: Tag): number {
  return Object.values(tag.uses).reduce((sum, count) => sum + count, 0);
}

/** The ruling in the owner's words, or what a withdrawal said instead. */
function words(decision: Decision): string {
  if (decision.state === "settled") {
    return `“${decision.ruling?.text ?? ""}”`;
  }
  const reason = decision.withdrawn_reason;
  return reason === undefined || reason === "" ? "Withdrawn" : `Withdrawn — “${reason}”`;
}

/** The option key alone — the ruling's own words already say what it meant. */
function optionKey(decision: Decision): string | undefined {
  if (decision.state !== "settled") {
    return undefined;
  }
  return decision.options.find((option) => option.key === decision.ruling?.option)?.key;
}

/** One row of filter chips, with the chip that is on clicking off again. */
function ChipRow({ label, allLabel, chips, chosen, onChoose }: ChipRowProps): ReactElement | null {
  if (chips.length === 0) {
    return null;
  }
  return (
    <div className="cc-registry-chips">
      <span className="cc-chip-label">{label}</span>
      <button
        type="button"
        className={`cc-filter-chip${chosen === undefined ? " cc-filter-chip-on" : ""}`}
        onClick={() => {
          onChoose(undefined);
        }}
      >
        {allLabel}
      </button>
      {chips.map((chip) => (
        <button
          key={chip.id}
          type="button"
          className={`cc-filter-chip${chosen === chip.id ? " cc-filter-chip-on" : ""}`}
          onClick={() => {
            onChoose(chosen === chip.id ? undefined : chip.id);
          }}
        >
          {chip.label} {chip.count}
        </button>
      ))}
    </div>
  );
}

function LedgerRow({ decision, selected, onOpen, projectName }: LedgerRowProps): ReactElement {
  const option = optionKey(decision);
  const rowRef = useScrollSelectedIntoView<HTMLButtonElement>(selected);
  const withdrawn = decision.state === "withdrawn" ? " cc-ledger-withdrawn" : "";
  return (
    <button
      ref={rowRef}
      type="button"
      className={`cc-ledger-row${withdrawn}${selected ? " cc-ledger-row-selected" : ""}`}
      aria-current={selected ? "true" : undefined}
      onClick={() => {
        onOpen(decision.id);
      }}
    >
      <span className="cc-ledger-date">{fmtDay(filedAt(decision))}</span>
      <div className="cc-ledger-main">
        <div className="cc-ledger-title">{decision.title}</div>
        <div className="cc-ledger-words">
          {option === undefined ? null : <span className="cc-ledger-option">{option}</span>}
          {words(decision)}
        </div>
        <div className="cc-ledger-meta">
          <BotAvatar
            avatar={decision.raised_by.avatar}
            name={decision.raised_by.name}
            id={decision.raised_by.bot_id}
            size="sm"
          />
          <span>{decision.raised_by.name}</span>
          <span>·</span>
          <span>{projectName(decision.project_id)}</span>
          <span>·</span>
          <span className="cc-mono">{decision.tags.join(" ")}</span>
        </div>
      </div>
    </button>
  );
}

/**
 * The settled ledger: every ruling the owner has given, newest first.
 *
 * Withdrawn decisions stay in it. A registry that quietly drops what was
 * called off is a registry a bot cannot trust to be the whole record.
 */
export default function Registry({
  rows,
  settledCount,
  tags,
  filter,
  onQuery,
  onTagFilter,
  onProjectFilter,
  onBotFilter,
  searchRef,
  cursorId,
  onOpen,
  projectName,
}: RegistryProps): ReactElement {
  // Memoised because the whole Control center re-renders on the minute clock
  // tick, and filtering plus grouping the ledger is the most expensive thing
  // on this tab — none of it depends on the time.
  const { groups, shown } = useMemo(() => {
    const filtered = rows.filter((row) => matchesRegistry(row, filter));
    return {
      groups: groupByMonth(filtered),
      shown: filtered.filter((row) => row.state === "settled").length,
    };
  }, [rows, filter]);
  const facets = useMemo(
    () => registryFacets(rows, filter, projectName),
    [rows, filter, projectName],
  );
  const tagChips = useMemo(
    () =>
      tags
        .filter((tag) => tag.retired_at === undefined)
        .map((tag) => ({ id: tag.name, label: tag.name, count: totalUses(tag) })),
    [tags],
  );

  return (
    <section className="cc-registry">
      <div className="cc-registry-inner">
        <div className="cc-registry-search">
          <input
            ref={searchRef}
            aria-label="Search rulings"
            placeholder="Search rulings, titles, bots…  /"
            value={filter.query}
            onChange={(event) => {
              onQuery(event.target.value);
            }}
          />
          <span className="cc-registry-summary">
            {shown} of {settledCount} rulings
          </span>
          <span className="cc-list-keys">
            <span className="cc-key">↑</span>
            <span className="cc-key">↓</span>
            <span className="cc-key">↩</span>
          </span>
        </div>

        <ChipRow
          label="tag"
          allLabel="All"
          chips={tagChips}
          chosen={filter.tagFilter}
          onChoose={onTagFilter}
        />
        <ChipRow
          label="project"
          allLabel="All projects"
          chips={facets.projects}
          chosen={filter.projectFilter}
          onChoose={onProjectFilter}
        />
        <ChipRow
          label="bot"
          allLabel="All bots"
          chips={facets.bots}
          chosen={filter.botFilter}
          onChoose={onBotFilter}
        />

        {groups.map((group) => (
          <Fragment key={group.label}>
            <div className="cc-month">{group.label}</div>
            {group.decisions.map((decision) => (
              <LedgerRow
                key={decision.id}
                decision={decision}
                selected={decision.id === cursorId}
                onOpen={onOpen}
                projectName={projectName}
              />
            ))}
          </Fragment>
        ))}

        {groups.length === 0 ? <div className="cc-registry-empty">No rulings match.</div> : null}
      </div>
    </section>
  );
}
