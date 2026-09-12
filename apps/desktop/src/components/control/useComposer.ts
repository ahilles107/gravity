import { useCallback, useState } from "react";

interface ComposerState {
  /** Which decision the words belong to; a different one starts blank. */
  readonly forId: string | undefined;
  readonly text: string;
  readonly pick: string | undefined;
  readonly holdOpen: boolean;
  readonly holdDate: string;
  readonly holdNote: string;
}

export interface Composer {
  readonly text: string;
  readonly pick: string | undefined;
  readonly holdOpen: boolean;
  readonly holdDate: string;
  readonly holdNote: string;
  readonly setText: (text: string) => void;
  /** Picks an option; picking it again clears it. */
  readonly togglePick: (key: string) => void;
  readonly clearPick: () => void;
  readonly setHoldOpen: (open: boolean) => void;
  readonly setHoldDate: (date: string) => void;
  readonly setHoldNote: (note: string) => void;
  /** What a discarded draft said, put back so it can be edited. */
  readonly prefill: (text: string, pick: string | undefined) => void;
  readonly clear: () => void;
}

function blank(forId: string | undefined): ComposerState {
  return { forId, text: "", pick: undefined, holdOpen: false, holdDate: "", holdNote: "" };
}

/**
 * The words being typed for one decision.
 *
 * Moving to another decision starts blank: a half-written ruling must never
 * be saved against the wrong question. The reset happens during render,
 * keyed on the id, rather than in an effect.
 */
export function useComposer(selectedId: string | undefined): Composer {
  const [stored, setStored] = useState<ComposerState>(() => blank(selectedId));
  const state = stored.forId === selectedId ? stored : blank(selectedId);
  if (stored.forId !== selectedId) {
    setStored(state);
  }

  const update = useCallback(
    (patch: Partial<ComposerState>): void => {
      setStored((prev) => ({
        ...(prev.forId === selectedId ? prev : blank(selectedId)),
        ...patch,
      }));
    },
    [selectedId],
  );

  const togglePick = useCallback(
    (key: string): void => {
      setStored((prev) => {
        const base = prev.forId === selectedId ? prev : blank(selectedId);
        return { ...base, pick: base.pick === key ? undefined : key };
      });
    },
    [selectedId],
  );

  return {
    text: state.text,
    pick: state.pick,
    holdOpen: state.holdOpen,
    holdDate: state.holdDate,
    holdNote: state.holdNote,
    setText: useCallback((text: string) => update({ text }), [update]),
    togglePick,
    clearPick: useCallback(() => update({ pick: undefined }), [update]),
    setHoldOpen: useCallback((holdOpen: boolean) => update({ holdOpen }), [update]),
    setHoldDate: useCallback((holdDate: string) => update({ holdDate }), [update]),
    setHoldNote: useCallback((holdNote: string) => update({ holdNote }), [update]),
    prefill: useCallback(
      (text: string, pick: string | undefined) => update({ text, pick }),
      [update],
    ),
    clear: useCallback(() => setStored(blank(selectedId)), [selectedId]),
  };
}
