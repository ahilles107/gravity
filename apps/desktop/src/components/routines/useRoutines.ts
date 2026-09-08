import { useCallback, useState } from "react";
import { capture, captureException } from "../../analytics";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { NotifyLevel, OverlapPolicy, Routine, RoutineTrigger } from "../../protocol/entities";
import { errText } from "../../util";

type Toast = (level: NotifyLevel, title: string, body: string) => void;

/** Optional execution bounds a routine may carry. */
export interface RoutineLimits {
  readonly maxDurationSeconds?: number;
  readonly maxAttempts?: number;
}

export interface RoutinesApi {
  readonly routines: readonly Routine[];
  readonly toggleEnabled: (routine: Routine) => Promise<void>;
  readonly runNow: (routine: Routine) => Promise<void>;
  readonly create: (
    name: string,
    trigger: RoutineTrigger,
    prompt: string,
    overlapPolicy: OverlapPolicy,
    limits: RoutineLimits,
  ) => Promise<boolean>;
}

/** A bot's routines plus the mutations the panel offers. */
export function useRoutines(
  client: DaemonApi,
  botId: string,
  connected: boolean,
  onChanged: (botId: string, routines: readonly Routine[]) => void,
  onToast: Toast,
): RoutinesApi {
  const [routines, setRoutines] = useState<readonly Routine[]>([]);

  const apply = useCallback(
    (next: readonly Routine[]): void => {
      setRoutines(next);
      onChanged(botId, next);
    },
    [botId, onChanged],
  );

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request({ type: "list_routines", bot_id: botId }, "routines");
      apply(reply.routines);
    } catch (error) {
      captureException(error, "routine_list");
      onToast("error", "Failed to load routines", errText(error));
    }
  }, [apply, botId, client, onToast]);

  useLoadOnConnect(connected, load);

  const toggleEnabled = useCallback(
    async (routine: Routine): Promise<void> => {
      try {
        const reply = await client.request(
          {
            type: "set_routine_enabled",
            routine_id: routine.id,
            enabled: !routine.enabled,
          },
          "routine",
        );
        capture("routine_toggled", { enabled: reply.routine.enabled });
        apply(routines.map((item) => (item.id === reply.routine.id ? reply.routine : item)));
      } catch (error) {
        captureException(error, "routine_toggle");
        onToast("error", "Failed to update routine", errText(error));
      }
    },
    [apply, client, onToast, routines],
  );

  const runNow = useCallback(
    async (routine: Routine): Promise<void> => {
      try {
        await client.request({ type: "run_routine_now", routine_id: routine.id }, "ok");
        capture("routine_run", {});
        onToast("info", "Routine queued", `${routine.name} will run now.`);
      } catch (error) {
        captureException(error, "routine_run");
        onToast("error", "Run now failed", errText(error));
      }
    },
    [client, onToast],
  );

  const create = useCallback(
    async (
      name: string,
      trigger: RoutineTrigger,
      prompt: string,
      overlapPolicy: OverlapPolicy,
      limits: RoutineLimits,
    ): Promise<boolean> => {
      try {
        const reply = await client.request(
          {
            type: "create_routine",
            bot_id: botId,
            name,
            trigger,
            prompt,
            overlap_policy: overlapPolicy,
            ...(limits.maxDurationSeconds === undefined
              ? {}
              : { max_duration_seconds: limits.maxDurationSeconds }),
            ...(limits.maxAttempts === undefined ? {} : { max_attempts: limits.maxAttempts }),
          },
          "routine",
        );
        capture("routine_created", { trigger_type: trigger.kind });
        apply([reply.routine, ...routines]);
        return true;
      } catch (error) {
        captureException(error, "routine_create");
        onToast("error", "Create routine failed", errText(error));
        return false;
      }
    },
    [apply, botId, client, onToast, routines],
  );

  return { routines, toggleEnabled, runNow, create };
}
