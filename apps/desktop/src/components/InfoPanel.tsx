import { FolderOpen } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../protocol/api";
import type { Bot, NotifyLevel } from "../protocol/entities";
import { revealBotWorkspace } from "../reveal";
import { errText } from "../util";
import BotAvatar from "./BotAvatar";
import { BOT_ICONS } from "./botIcons";
import BotHistory from "./bot/BotHistory";

interface InfoPanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onBotUpdated: (bot: Bot) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

interface Draft {
  readonly name: string;
  readonly avatar: string;
  readonly description: string;
  readonly instructions: string;
}

function draftOf(bot: Bot): Draft {
  return {
    name: bot.name,
    avatar: bot.avatar,
    description: bot.description,
    instructions: bot.instructions,
  };
}

/**
 * Applies bot-driven changes to fields the user has not edited. A bot can
 * rename or re-avatar itself while this inspector is open, but an unrelated
 * push must not erase a local draft that has not been saved yet.
 */
function reconcileDraft(previousBot: Bot, nextBot: Bot, current: Draft): Draft {
  if (previousBot.id !== nextBot.id) {
    return draftOf(nextBot);
  }
  const previous = draftOf(previousBot);
  const next = draftOf(nextBot);
  const reconciled: Draft = {
    name: current.name === previous.name ? next.name : current.name,
    avatar: current.avatar === previous.avatar ? next.avatar : current.avatar,
    description:
      current.description === previous.description ? next.description : current.description,
    instructions:
      current.instructions === previous.instructions ? next.instructions : current.instructions,
  };
  if (
    reconciled.name === current.name &&
    reconciled.avatar === current.avatar &&
    reconciled.description === current.description &&
    reconciled.instructions === current.instructions
  ) {
    return current;
  }
  return reconciled;
}

/**
 * Only the fields that actually differ are sent. The daemon records a revision
 * per changed field, so submitting untouched values would fill a bot's history
 * with entries the user never made.
 */
function changedFields(bot: Bot, draft: Draft): Partial<Draft> {
  const original = draftOf(bot);
  const changed: Record<string, string> = {};
  for (const key of Object.keys(original) as (keyof Draft)[]) {
    if (draft[key] !== original[key]) {
      changed[key] = draft[key];
    }
  }
  return changed;
}

/** Bot editor: name, avatar, description and instructions, plus history. */
export default function InfoPanel({
  client,
  bot,
  connected,
  canControl,
  onBotUpdated,
  onToast,
}: InfoPanelProps): ReactElement {
  const [draft, setDraft] = useState<Draft>(() => draftOf(bot));
  const [saving, setSaving] = useState(false);
  const previousBotRef = useRef(bot);

  useEffect(() => {
    const previousBot = previousBotRef.current;
    previousBotRef.current = bot;
    setDraft((current) => reconcileDraft(previousBot, bot, current));
  }, [bot]);

  const changed = changedFields(bot, draft);
  const dirty = Object.keys(changed).length > 0;

  const set = (field: keyof Draft, value: string): void => {
    setDraft((current) => ({ ...current, [field]: value }));
  };

  const save = async (): Promise<void> => {
    setSaving(true);
    try {
      const reply = await client.request({ type: "update_bot", bot_id: bot.id, ...changed }, "bot");
      onBotUpdated(reply.bot);
      setDraft(draftOf(reply.bot));
      onToast("info", "Bot updated", `${reply.bot.name} saved.`);
    } catch (error) {
      onToast("error", "Update failed", errText(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="panel">
      <h3 className="panel-title">Bot info</h3>

      <div className="bot-identity">
        <BotAvatar avatar={draft.avatar} name={draft.name} id={bot.id} size="lg" />
        <dl className="info-meta">
          <dt className="workspace-heading">
            <span>Workspace</span>
            <button
              type="button"
              className="workspace-reveal"
              aria-label="Reveal bot workspace in Finder"
              title="Reveal in Finder"
              onClick={() => {
                void revealBotWorkspace(bot.workspace_path);
              }}
            >
              <FolderOpen size={14} strokeWidth={1.75} aria-hidden="true" />
            </button>
          </dt>
          <dd className="mono">{bot.workspace_path}</dd>
          <dt>State</dt>
          <dd>
            {bot.state}
            {bot.state_reason.length > 0 ? ` — ${bot.state_reason}` : ""}
          </dd>
          <dt>Created</dt>
          <dd>{bot.created_at}</dd>
          {bot.created_by_bot_id != null && bot.created_by_bot_id.length > 0 ? (
            <>
              <dt>Origin</dt>
              <dd>Created by another bot</dd>
            </>
          ) : null}
        </dl>
      </div>

      <label className="field">
        <span className="field-label">Name</span>
        <input
          type="text"
          disabled={!canControl}
          value={draft.name}
          onChange={(event) => {
            set("name", event.target.value);
          }}
        />
        <span className="field-hint">
          Other bots address this bot by name. Renaming tells them the new one.
        </span>
      </label>

      <div className="field">
        <span className="field-label" id="avatar-label">
          Avatar
        </span>
        <div className="avatar-picker" role="radiogroup" aria-labelledby="avatar-label">
          {Object.entries(BOT_ICONS).map(([icon, src]) => {
            const value = `icon:${icon}`;
            return (
              <button
                key={icon}
                type="button"
                role="radio"
                aria-checked={draft.avatar === value}
                aria-label={icon}
                className={`avatar-choice${draft.avatar === value ? " avatar-choice-on" : ""}`}
                disabled={!canControl}
                onClick={() => {
                  set("avatar", value);
                }}
              >
                <img src={src} alt="" />
              </button>
            );
          })}
        </div>
        <span className="field-hint">Every bot is dealt one of these at random when created.</span>
      </div>

      <label className="field">
        <span className="field-label">Description</span>
        <textarea
          rows={3}
          disabled={!canControl}
          value={draft.description}
          onChange={(event) => {
            set("description", event.target.value);
          }}
        />
      </label>

      <label className="field">
        <span className="field-label">Instructions</span>
        <textarea
          rows={10}
          disabled={!canControl}
          placeholder="Standing instructions for this bot."
          value={draft.instructions}
          onChange={(event) => {
            set("instructions", event.target.value);
          }}
        />
        <span className="field-hint">
          Appended to the bot&apos;s system prompt. A running bot is told about changes straight
          away and does not need restarting.
        </span>
      </label>

      <div className="panel-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={!connected || saving || !canControl || !dirty}
          onClick={() => {
            void save();
          }}
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>

      <BotHistory
        client={client}
        bot={bot}
        canControl={canControl}
        onBotUpdated={onBotUpdated}
        onToast={onToast}
      />
    </div>
  );
}
