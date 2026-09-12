import { useCallback, useState } from "react";
import type { Decision } from "../../protocol/decisions";
import { dayToIso } from "./decisions";
import { buildPublishItems } from "./publishPlan";
import type { Composer } from "./useComposer";
import type { ControlState } from "./useControlState";
import type { DecisionsApi } from "./useDecisions";
import type { NotifySets } from "./useNotifySets";

export interface ControlActions {
  /** A request is in flight; the composer and the tray hold still meanwhile. */
  readonly busy: boolean;
  readonly saveRuling: () => void;
  readonly askInThread: () => void;
  readonly hold: () => void;
  readonly editDraft: () => void;
  readonly publishAll: () => void;
}

interface ActionDeps {
  readonly api: DecisionsApi;
  readonly state: ControlState;
  readonly composer: Composer;
  readonly notify: NotifySets;
  readonly reading: Decision | undefined;
  readonly canControl: boolean;
}

/** The verbs the composer, the draft box and the tray offer, bound to what is being read. */
export function useControlActions(deps: ActionDeps): ControlActions {
  const { api, state, composer, notify, reading, canControl } = deps;
  const [busy, setBusy] = useState(false);

  const run = useCallback(async (work: () => Promise<unknown>): Promise<void> => {
    setBusy(true);
    try {
      await work();
    } finally {
      setBusy(false);
    }
  }, []);

  /** The open decision being read and the trimmed words, or nothing to act on. */
  const openWithWords = useCallback((): { id: string; text: string } | undefined => {
    const text = composer.text.trim();
    if (reading === undefined || reading.state !== "open" || text === "" || !canControl || busy) {
      return undefined;
    }
    return { id: reading.id, text };
  }, [busy, canControl, composer.text, reading]);

  // An option picked without words is still a ruling: the daemon and the bots
  // need a sentence to quote, so the option's own label stands in for one.
  const saveRuling = useCallback((): void => {
    if (reading === undefined || reading.state !== "open" || !canControl || busy) {
      return;
    }
    const picked = reading.options.find((option) => option.key === composer.pick);
    const text = composer.text.trim() || picked?.label || "";
    if (text === "") {
      return;
    }
    void run(async () => {
      if (await api.answer(reading.id, text, composer.pick)) {
        composer.clear();
      }
    });
  }, [api, busy, canControl, composer, reading, run]);

  const askInThread = useCallback((): void => {
    const target = openWithWords();
    if (target === undefined) {
      return;
    }
    void run(async () => {
      const comment = await api.comment(target.id, target.text);
      if (comment !== undefined) {
        api.appendComment(comment);
        composer.setText("");
      }
    });
  }, [api, composer, openWithWords, run]);

  const hold = useCallback((): void => {
    if (reading === undefined || reading.state !== "open" || !canControl || busy) {
      return;
    }
    const next = api.waiting.find((item) => item.id !== reading.id);
    void run(async () => {
      if (await api.hold(reading.id, composer.holdNote, dayToIso(composer.holdDate))) {
        composer.clear();
        if (next !== undefined) {
          state.select(next.id);
        }
      }
    });
  }, [api, busy, canControl, composer, reading, run, state]);

  // The daemon clears a discarded draft, so its words are captured first and
  // put back into the composer once the discard has landed.
  const editDraft = useCallback((): void => {
    if (reading?.ruling === undefined || !canControl) {
      return;
    }
    const { text, option } = reading.ruling;
    void run(async () => {
      if (await api.unanswer(reading.id)) {
        composer.prefill(text, option);
      }
    });
  }, [api, canControl, composer, reading, run]);

  const publishAll = useCallback((): void => {
    if (!canControl || api.drafts.length === 0) {
      return;
    }
    void run(async () => {
      if (await api.publish(buildPublishItems(api.drafts, notify.get))) {
        state.setTrayOpen(false);
      }
    });
  }, [api, canControl, notify.get, run, state]);

  return { busy, saveRuling, askInThread, hold, editDraft, publishAll };
}
