import { useCallback, useEffect, useMemo, useState } from "react";
import { captureException } from "../../analytics";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Decision, DecisionComment, Tag } from "../../protocol/decisions";
import type { NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";
import { sortByFiledAt, sortWaiting } from "./decisions";
import { useDecisionMutations } from "./useDecisionMutations";
import type { DecisionMutations } from "./useDecisionMutations";

type Toast = (level: NotifyLevel, title: string, body: string) => void;

/** The daemon clamps `limit` here; the default would silently cap the ledger. */
const PAGE = 500;

/**
 * Pages to walk per list before giving up and saying so.
 *
 * A cap of one page was the bug this replaces: past 500 the ledger simply
 * stopped, and the "3 of 41" summary was computed from the truncated store so
 * it agreed with itself. A ceiling still has to exist — this one is high
 * enough that reaching it means something is wrong, and it is reported rather
 * than silent.
 */
const MAX_PAGES = 20;

const LISTS = ["open", "held", "settled", "withdrawn"] as const;

export interface DecisionsApi extends DecisionMutations {
  /** Every record loaded, by id, whatever its state. */
  readonly byId: ReadonlyMap<string, Decision>;
  /** Open and drafted, in the order the list shows. */
  readonly waiting: readonly Decision[];
  readonly drafts: readonly Decision[];
  readonly held: readonly Decision[];
  /** Settled and withdrawn, newest first. */
  readonly registry: readonly Decision[];
  readonly settled: readonly Decision[];
  readonly tags: readonly Tag[];
  /** False until every list has landed, so the view can tell empty from loading. */
  readonly loaded: boolean;
  readonly reload: () => Promise<void>;
  readonly reloadTags: () => Promise<void>;
  /** Takes the daemon's record into the store, keeping a fuller thread if it has one. */
  readonly replace: (decision: Decision) => void;
  readonly appendComment: (comment: DecisionComment) => void;
}

/**
 * The registry as the Control center sees it: one store, partitioned by state.
 *
 * Four lists rather than `all`, because `all` excludes withdrawn and sorts by
 * deadline, so a cap would drop the newest rulings first.
 */
export function useDecisions(client: DaemonApi, connected: boolean, onToast: Toast): DecisionsApi {
  const [byId, setById] = useState<ReadonlyMap<string, Decision>>(new Map());
  const [tags, setTags] = useState<readonly Tag[]>([]);
  const [loaded, setLoaded] = useState(false);

  /**
   * Every record in one state, walking the daemon's cursor.
   *
   * `before` is a composite of the same key the server sorts by, so a page
   * boundary never skips or repeats a row two decisions share a timestamp
   * with. Returns whether the ceiling was hit before the list ran out.
   */
  const loadList = useCallback(
    async (
      state: (typeof LISTS)[number],
    ): Promise<{
      decisions: Decision[];
      complete: boolean;
    }> => {
      const decisions: Decision[] = [];
      let before: string | undefined;
      for (let page = 0; page < MAX_PAGES; page += 1) {
        // Sequential by nature: the next page's cursor is the last id of this
        // one, so there is nothing to run in parallel.
        // oxlint-disable-next-line no-await-in-loop
        const reply = await client.request(
          {
            type: "list_decisions",
            state,
            limit: PAGE,
            ...(before === undefined ? {} : { before }),
          },
          "decisions",
        );
        decisions.push(...reply.decisions);
        if (reply.decisions.length < PAGE) {
          return { decisions, complete: true };
        }
        before = reply.decisions.at(-1)?.id;
        if (before === undefined) {
          return { decisions, complete: true };
        }
      }
      return { decisions, complete: false };
    },
    [client],
  );

  const reload = useCallback(async (): Promise<void> => {
    try {
      const replies = await Promise.all(LISTS.map(loadList));
      setById((prev) => {
        const next = new Map<string, Decision>();
        for (const reply of replies) {
          for (const decision of reply.decisions) {
            next.set(decision.id, merge(prev.get(decision.id), decision));
          }
        }
        return next;
      });
      setLoaded(true);
      if (replies.some((reply) => !reply.complete)) {
        onToast(
          "warn",
          "The registry is larger than this view loads",
          `Showing the first ${MAX_PAGES * PAGE} records per list. Narrow by tag, or search from a bot with list_decisions.`,
        );
      }
    } catch (error) {
      captureException(error, "decision_list");
      onToast("error", "Failed to load decisions", errText(error));
    }
  }, [loadList, onToast]);

  const reloadTags = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request({ type: "list_tags" }, "tags");
      setTags(reply.tags);
    } catch (error) {
      captureException(error, "tag_list");
    }
  }, [client]);

  const load = useCallback(async (): Promise<void> => {
    await Promise.all([reload(), reloadTags()]);
  }, [reload, reloadTags]);

  useLoadOnConnect(connected, load);

  const replace = useCallback((decision: Decision): void => {
    setById((prev) => new Map(prev).set(decision.id, merge(prev.get(decision.id), decision)));
  }, []);

  const forget = useCallback((decisionId: string): void => {
    setById((prev) => {
      const next = new Map(prev);
      next.delete(decisionId);
      return next;
    });
  }, []);

  const appendComment = useCallback((comment: DecisionComment): void => {
    setById((prev) => {
      const current = prev.get(comment.decision_id);
      if (current === undefined) {
        return prev;
      }
      return new Map(prev).set(comment.decision_id, withComment(current, comment));
    });
  }, []);

  // A decision the daemon changed under us — a bot commented, a deadline
  // warning fired, another client published — arrives here rather than
  // waiting for the next manual refresh.
  useEffect(() => {
    const off = [
      client.on("decision_update", (push) => {
        replace(push.decision);
      }),
      client.on("decision_deleted", (push) => {
        forget(push.decision_id);
      }),
      client.on("decision_comment_new", (push) => {
        appendComment(push.comment);
      }),
    ];
    return () => {
      for (const unsubscribe of off) {
        unsubscribe();
      }
    };
  }, [appendComment, client, forget, replace]);

  const mutations = useDecisionMutations({ client, onToast, replace, forget, reload, reloadTags });

  const lists = useMemo(() => partition(byId), [byId]);

  return {
    byId,
    ...lists,
    tags,
    loaded,
    reload,
    reloadTags,
    replace,
    appendComment,
    ...mutations,
  };
}

interface DecisionLists {
  readonly waiting: readonly Decision[];
  readonly drafts: readonly Decision[];
  readonly held: readonly Decision[];
  readonly registry: readonly Decision[];
  readonly settled: readonly Decision[];
}

/** Splits the store into the lists the view renders, each in its own order. */
function partition(byId: ReadonlyMap<string, Decision>): DecisionLists {
  const all = [...byId.values()];
  const waiting = sortWaiting(
    all.filter((item) => item.state === "open" || item.state === "answered"),
  );
  const registry = sortByFiledAt(
    all.filter((item) => item.state === "settled" || item.state === "withdrawn"),
  );
  return {
    waiting,
    drafts: waiting.filter((item) => item.state === "answered"),
    held: sortWaiting(all.filter((item) => item.state === "held")),
    registry,
    settled: registry.filter((item) => item.state === "settled"),
  };
}

/**
 * List replies and most pushes omit the thread and the notifications; a full
 * record fetched earlier must not be flattened by a summary that came later.
 */
function merge(prev: Decision | undefined, next: Decision): Decision {
  if (prev === undefined) {
    return next;
  }
  return {
    ...next,
    ...(next.comments === undefined && prev.comments !== undefined
      ? { comments: prev.comments }
      : {}),
    ...(next.notifications === undefined && prev.notifications !== undefined
      ? { notifications: prev.notifications }
      : {}),
  };
}

// An empty thread is omitted from the wire rather than sent as `[]`, so an
// absent `comments` on a record with no comments is a thread that is known to
// be empty, not one that was never fetched — the first comment has to land in
// it or the reading pane stays blank until the record is fetched again.
function withComment(decision: Decision, comment: DecisionComment): Decision {
  const comments = decision.comments ?? (decision.comment_count === 0 ? [] : undefined);
  if (comments === undefined) {
    return { ...decision, comment_count: decision.comment_count + 1 };
  }
  if (comments.some((item) => item.id === comment.id)) {
    return decision;
  }
  return {
    ...decision,
    comment_count: decision.comment_count + 1,
    last_comment_at: comment.created_at,
    comments: [...comments, comment],
  };
}
