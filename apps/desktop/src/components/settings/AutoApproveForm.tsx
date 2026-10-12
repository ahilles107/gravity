import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { DaemonConfig, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

interface AutoApproveFormProps {
  readonly client: DaemonApi;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly rules: readonly string[];
  readonly onSaved: (config: DaemonConfig) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/** One rule per line; blank lines are dropped, matching what the daemon stores. */
function parseRules(draft: string): readonly string[] {
  return draft
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/**
 * Permission prompts the daemon answers by pressing Enter (the dialog's
 * default "Yes"). Each rule is matched, ignoring case, anywhere in the text of
 * Claude Code's permission notification; `*` matches any run of characters.
 */
export default function AutoApproveForm(props: AutoApproveFormProps): ReactElement {
  const { client, connected, canControl, rules, onSaved, onToast } = props;
  const [draft, setDraft] = useState(rules.join("\n"));
  const [saving, setSaving] = useState(false);

  const parsed = parseRules(draft);
  const dirty = parsed.join("\n") !== rules.join("\n");

  const save = async (): Promise<void> => {
    setSaving(true);
    try {
      const reply = await client.request(
        { type: "set_config", auto_approve_rules: parsed },
        "config",
      );
      onSaved(reply.config);
      setDraft(reply.config.auto_approve_rules.join("\n"));
      onToast("info", "Daemon updated", "Auto-approve rules apply to the next permission prompt.");
    } catch (err) {
      onToast("error", "Update failed", errText(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form
      className="settings-row settings-row-form"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <div className="settings-row-text">
        <label className="settings-row-label" htmlFor="settings-auto-approve">
          Auto-approve permission prompts
        </label>
        <div className="settings-row-help">
          One rule per line. When a bot asks for permission and the permission dialog on the bot's
          terminal contains a rule (case-insensitive, <code>*</code> matches anything), Gravity
          presses Enter to accept it. Example: <code>Slack - Read channel messages</code>. Empty by
          default; a rule of only <code>*</code> is refused. Rules apply to every bot.
        </div>
        <textarea
          id="settings-auto-approve"
          className="settings-textarea"
          rows={4}
          spellCheck={false}
          placeholder="Slack - Read channel messages"
          value={draft}
          disabled={!canControl}
          onChange={(event) => {
            setDraft(event.target.value);
          }}
        />
        {canControl ? (
          <div className="settings-inline-fields">
            <button
              type="submit"
              className="btn btn-small btn-primary"
              aria-label="Save auto-approve rules"
              disabled={!connected || saving || !dirty}
            >
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        ) : null}
      </div>
    </form>
  );
}
