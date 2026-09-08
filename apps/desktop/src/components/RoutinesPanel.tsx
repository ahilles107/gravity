import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../protocol/api";
import type { Bot, NotifyLevel, Routine } from "../protocol/entities";
import PanelHeader from "./PanelHeader";
import CreateRoutineForm from "./routines/CreateRoutineForm";
import RoutineItem from "./routines/RoutineItem";
import RunHistory from "./routines/RunHistory";
import { useRoutines } from "./routines/useRoutines";

interface RoutinesPanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly bots: readonly Bot[];
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onRoutinesChanged: (botId: string, routines: readonly Routine[]) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

export default function RoutinesPanel(props: RoutinesPanelProps): ReactElement {
  const { client, bot, bots, connected, canControl, onToast } = props;
  const api = useRoutines(client, bot.id, connected, props.onRoutinesChanged, onToast);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  return (
    <div className="panel">
      <PanelHeader title="Routines">
        {canControl ? (
          <button
            type="button"
            className="btn btn-small btn-primary"
            disabled={!connected}
            onClick={() => {
              setCreating((prev) => !prev);
            }}
          >
            + New routine
          </button>
        ) : null}
      </PanelHeader>

      {creating ? (
        <CreateRoutineForm
          bots={bots.filter((candidate) => candidate.id !== bot.id)}
          onSubmit={api.create}
          onClose={() => {
            setCreating(false);
          }}
        />
      ) : null}

      {api.routines.length === 0 && !creating ? (
        <div className="muted">No routines yet.</div>
      ) : null}

      <ul className="routine-list">
        {api.routines.map((routine) => (
          <RoutineItem
            key={routine.id}
            routine={routine}
            bots={bots}
            connected={connected}
            canControl={canControl}
            expanded={expanded === routine.id}
            onToggleEnabled={() => {
              void api.toggleEnabled(routine);
            }}
            onRunNow={() => {
              void api.runNow(routine);
            }}
            onToggleHistory={() => {
              setExpanded((prev) => (prev === routine.id ? null : routine.id));
            }}
          >
            {expanded === routine.id ? (
              <RunHistory
                client={client}
                routineId={routine.id}
                connected={connected}
                canControl={canControl}
              />
            ) : null}
          </RoutineItem>
        ))}
      </ul>
    </div>
  );
}
