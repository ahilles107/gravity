import { useCallback } from "react";
import { capture, captureException } from "../../analytics";
import type { DaemonApi } from "../../protocol/api";
import type { PublishItem } from "../../protocol/decisionRequests";
import type { Decision, DecisionComment } from "../../protocol/decisions";
import type { NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

type Toast = (level: NotifyLevel, title: string, body: string) => void;

/** Every way the owner acts on a decision. */
export interface DecisionMutations {
  readonly answer: (
    decisionId: string,
    text: string,
    option?: string,
    reason?: string,
  ) => Promise<boolean>;
  /** Discards a draft; returns true so the caller can prefill the composer with what it had. */
  readonly unanswer: (decisionId: string) => Promise<boolean>;
  readonly hold: (decisionId: string, comment?: string, until?: string) => Promise<boolean>;
  readonly resume: (decisionId: string) => Promise<void>;
  readonly confirm: (decisionId: string) => Promise<void>;
  readonly comment: (decisionId: string, body: string) => Promise<DecisionComment | undefined>;
  readonly publish: (items: readonly PublishItem[]) => Promise<boolean>;
  readonly setTags: (decisionId: string, tags: readonly string[]) => Promise<void>;
  readonly remove: (decisionId: string) => Promise<void>;
}

interface MutationDeps {
  readonly client: DaemonApi;
  readonly onToast: Toast;
  /** Takes the daemon's record of a decision into the list. */
  readonly replace: (decision: Decision) => void;
  readonly forget: (decisionId: string) => void;
  readonly reload: () => Promise<void>;
  readonly reloadTags: () => Promise<void>;
}

/**
 * Every mutation is reply-driven rather than optimistic: a ruling is the one
 * thing in this app that must never look like it landed when it did not.
 */
export function useDecisionMutations(deps: MutationDeps): DecisionMutations {
  const { client, onToast, replace, forget, reload, reloadTags } = deps;

  const answer = useCallback(
    async (
      decisionId: string,
      text: string,
      option?: string,
      reason?: string,
    ): Promise<boolean> => {
      try {
        const reply = await client.request(
          {
            type: "answer_decision",
            decision_id: decisionId,
            ruling_text: text,
            ...(option === undefined ? {} : { ruling_option: option }),
            ...(reason === undefined || reason.trim() === "" ? {} : { ruling_reason: reason }),
          },
          "decision",
        );
        capture("decision_answered", { picked_option: option !== undefined });
        replace(reply.decision);
        return true;
      } catch (error) {
        captureException(error, "decision_answer");
        onToast("error", "Failed to save the ruling", errText(error));
        return false;
      }
    },
    [client, onToast, replace],
  );

  const unanswer = useCallback(
    async (decisionId: string): Promise<boolean> => {
      try {
        const reply = await client.request(
          { type: "unanswer_decision", decision_id: decisionId },
          "decision",
        );
        replace(reply.decision);
        return true;
      } catch (error) {
        captureException(error, "decision_update");
        onToast("error", "Failed to reopen the draft", errText(error));
        return false;
      }
    },
    [client, onToast, replace],
  );

  const hold = useCallback(
    async (decisionId: string, comment?: string, until?: string): Promise<boolean> => {
      try {
        const reply = await client.request(
          {
            type: "hold_decision",
            decision_id: decisionId,
            ...(comment === undefined || comment.trim() === "" ? {} : { comment }),
            ...(until === undefined ? {} : { until }),
          },
          "decision",
        );
        capture("decision_held", {});
        replace(reply.decision);
        return true;
      } catch (error) {
        captureException(error, "decision_hold");
        onToast("error", "Failed to hold the decision", errText(error));
        return false;
      }
    },
    [client, onToast, replace],
  );

  const simple = useCallback(
    async (
      type: "resume_decision" | "confirm_decision",
      decisionId: string,
      failure: string,
    ): Promise<void> => {
      try {
        const reply = await client.request({ type, decision_id: decisionId }, "decision");
        replace(reply.decision);
      } catch (error) {
        captureException(error, "decision_update");
        onToast("error", failure, errText(error));
      }
    },
    [client, onToast, replace],
  );

  const resume = useCallback(
    (decisionId: string) => simple("resume_decision", decisionId, "Failed to resume"),
    [simple],
  );

  const confirm = useCallback(
    (decisionId: string) => simple("confirm_decision", decisionId, "Failed to confirm"),
    [simple],
  );

  const comment = useCallback(
    async (decisionId: string, body: string): Promise<DecisionComment | undefined> => {
      try {
        const reply = await client.request(
          { type: "comment_decision", decision_id: decisionId, body },
          "decision_comment",
        );
        return reply.comment;
      } catch (error) {
        captureException(error, "decision_comment");
        onToast("error", "Failed to post the comment", errText(error));
        return undefined;
      }
    },
    [client, onToast],
  );

  const publish = useCallback(
    async (items: readonly PublishItem[]): Promise<boolean> => {
      try {
        const reply = await client.request({ type: "publish_decisions", items }, "publish_result");
        const notified = reply.results.reduce((sum, item) => sum + item.notified.length, 0);
        capture("decision_published", { count: reply.results.length, notified });
        const skipped = reply.results.flatMap((item) => item.skipped);
        if (skipped.length > 0) {
          // Which bot and why, rather than quietly telling fewer than the
          // owner ticked.
          onToast(
            "warn",
            "Some bots were not told",
            skipped.map((item) => `${item.bot}: ${item.reason}`).join("; "),
          );
        }
        await reload();
        return true;
      } catch (error) {
        captureException(error, "decision_publish");
        onToast("error", "Publish failed", errText(error));
        return false;
      }
    },
    [client, onToast, reload],
  );

  const setTags = useCallback(
    async (decisionId: string, tags: readonly string[]): Promise<void> => {
      try {
        const reply = await client.request(
          { type: "set_decision_tags", decision_id: decisionId, tags },
          "decision",
        );
        replace(reply.decision);
        await reloadTags();
      } catch (error) {
        captureException(error, "tag_update");
        onToast("error", "Failed to update tags", errText(error));
      }
    },
    [client, onToast, reloadTags, replace],
  );

  const remove = useCallback(
    async (decisionId: string): Promise<void> => {
      try {
        await client.request({ type: "delete_decision", decision_id: decisionId }, "ok");
        capture("decision_deleted", {});
        forget(decisionId);
      } catch (error) {
        captureException(error, "decision_delete");
        onToast("error", "Delete failed", errText(error));
      }
    },
    [client, forget, onToast],
  );

  return { answer, unanswer, hold, resume, confirm, comment, publish, setTags, remove };
}
