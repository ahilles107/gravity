import { useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactElement, TextareaHTMLAttributes } from "react";
import type { Tag } from "../../protocol/decisions";
import { fmtDay } from "./decisions";

interface TagsPanelProps {
  readonly tags: readonly Tag[];
  readonly canControl: boolean;
  readonly onAdd: (name: string) => Promise<boolean>;
  readonly onRename: (name: string, to: string) => Promise<boolean>;
  readonly onDescribe: (name: string, description: string) => Promise<boolean>;
  readonly onDelete: (name: string) => Promise<boolean>;
}

interface TagRowProps {
  readonly tag: Tag;
  readonly canControl: boolean;
  readonly confirming: boolean;
  readonly onConfirming: (name: string | undefined) => void;
  readonly onRename: (name: string, to: string) => Promise<boolean>;
  readonly onDescribe: (name: string, description: string) => Promise<boolean>;
  readonly onDelete: (name: string) => Promise<boolean>;
}

/** A tag is a bot-typed identifier, so it stays to what a bot can type back. */
const NAME = /^[a-z0-9-]{1,32}$/;

/** "Apple Ads" and "apple ads" are the same tag; a taxonomy only works if they are. */
function normalise(raw: string): string {
  return raw.trim().toLowerCase().replaceAll(/\s+/g, "-");
}

function totalUses(tag: Tag): number {
  return Object.values(tag.uses).reduce((sum, count) => sum + count, 0);
}

function plural(count: number): string {
  return count === 1 ? "" : "s";
}

function TagRow(props: TagRowProps): ReactElement {
  const { tag } = props;
  const [name, setName] = useState(tag.name);
  const [description, setDescription] = useState(tag.description);
  const total = totalUses(tag);

  const rename = async (): Promise<void> => {
    const next = normalise(name);
    // A name the daemon rejected, or one no bot could file under, snaps back
    // rather than sitting in the field looking saved.
    if (next === tag.name || !NAME.test(next) || !(await props.onRename(tag.name, next))) {
      setName(tag.name);
    }
  };

  const commitName = (): void => {
    void rename();
  };

  const commitDescription = (): void => {
    if (description === tag.description) {
      return;
    }
    void props.onDescribe(tag.name, description);
  };

  const onNameKey = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === "Enter") {
      commitName();
    }
    if (event.key === "Escape") {
      setName(tag.name);
    }
  };

  const onDescriptionKey = (event: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (event.key === "Enter") {
      event.preventDefault();
      commitDescription();
    }
    if (event.key === "Escape") {
      setDescription(tag.description);
    }
  };

  if (props.confirming) {
    return (
      <div className="cc-tag-row cc-tag-row-confirming">
        <div className="cc-tag-confirm">
          <span className="cc-tag-confirm-name">{tag.name}</span>
          <span className="cc-tag-confirm-text">
            {`Remove from ${total} decision${plural(total)}`}
            {tag.open_uses > 0 ? ` (${tag.open_uses} open)` : ""}? This cannot be undone.
          </span>
          <span className="cc-tag-confirm-actions">
            <button
              type="button"
              className="cc-btn-outline"
              onClick={() => {
                props.onConfirming(undefined);
              }}
            >
              Keep it
            </button>
            <button
              type="button"
              className="cc-btn-red"
              disabled={!props.canControl}
              onClick={() => {
                props.onConfirming(undefined);
                void props.onDelete(tag.name);
              }}
            >
              Delete tag
            </button>
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="cc-tag-row">
      <div className="cc-tag-main">
        <input
          className="cc-tag-name"
          aria-label={`Tag ${tag.name}`}
          value={name}
          disabled={!props.canControl}
          onChange={(event) => {
            setName(event.target.value);
          }}
          onBlur={commitName}
          onKeyDown={onNameKey}
        />
        <span className="cc-tag-count">
          {total === 0 ? "unused" : `${total} decision${plural(total)}`}
        </span>
        <span className="cc-tag-last">
          {tag.last_used_at === undefined ? "" : `last used ${fmtDay(tag.last_used_at)}`}
        </span>
        <button
          type="button"
          className="cc-tag-delete"
          disabled={!props.canControl}
          onClick={() => {
            props.onConfirming(tag.name);
          }}
        >
          Delete
        </button>
      </div>
      <GrowingTextarea
        className="cc-tag-desc"
        aria-label={`Description of ${tag.name}`}
        placeholder="What it means, in a sentence bots can read"
        value={description}
        disabled={!props.canControl}
        onChange={(event) => {
          setDescription(event.target.value);
        }}
        onBlur={commitDescription}
        onKeyDown={onDescriptionKey}
      />
    </div>
  );
}

type GrowingTextareaProps = Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "rows" | "value"> & {
  readonly value: string;
};

/** One line that grows with its text, so a description is never clipped. */
function GrowingTextarea(props: GrowingTextareaProps): ReactElement {
  const ref = useRef<HTMLTextAreaElement>(null);
  const { value } = props;
  useLayoutEffect(() => {
    const el = ref.current;
    if (el !== null && value.length >= 0) {
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [value]);
  return <textarea ref={ref} rows={1} {...props} />;
}

/**
 * The shared taxonomy.
 *
 * One list across every project, because a bot filing `spend` and another
 * filing `budget` for the same thing is what makes a registry unsearchable.
 * Renaming and deleting reach every decision, so deletion asks first and says
 * how many open decisions it would touch.
 */
export default function TagsPanel(props: TagsPanelProps): ReactElement {
  const [draft, setDraft] = useState("");
  const [confirming, setConfirming] = useState<string | undefined>(undefined);

  const live = props.tags.filter((tag) => tag.retired_at === undefined);
  // oxlint-disable-next-line unicorn/no-array-sort
  live.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

  const candidate = normalise(draft);
  const invalid = !NAME.test(candidate);

  const submit = async (): Promise<void> => {
    if (await props.onAdd(candidate)) {
      setDraft("");
    }
  };

  const add = (): void => {
    if (invalid || !props.canControl) {
      return;
    }
    void submit();
  };

  return (
    <section className="cc-tags">
      <div className="cc-tags-inner">
        <div className="cc-tags-head">
          <div>
            <div className="cc-tags-title">{live.length} tags</div>
            <div className="cc-tags-blurb">
              One taxonomy across every project. Renaming a tag renames it on every decision, open
              or settled. Bots read this list when they raise a decision.
            </div>
          </div>
          <div className="cc-tags-add">
            <input
              placeholder="new-tag"
              aria-label="New tag"
              value={draft}
              disabled={!props.canControl}
              onChange={(event) => {
                setDraft(event.target.value);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  add();
                }
              }}
            />
            <button
              type="button"
              className="cc-btn-accent"
              disabled={!props.canControl || invalid}
              onClick={add}
            >
              Add tag
            </button>
          </div>
        </div>

        <div className="cc-tags-list">
          {live.map((tag) => (
            <TagRow
              key={tag.id}
              tag={tag}
              canControl={props.canControl}
              confirming={confirming === tag.name}
              onConfirming={setConfirming}
              onRename={props.onRename}
              onDescribe={props.onDescribe}
              onDelete={props.onDelete}
            />
          ))}
        </div>

        <div className="cc-tags-foot">
          Deleting a tag removes it from every decision that carries it. Decisions are not deleted.
        </div>
      </div>
    </section>
  );
}
