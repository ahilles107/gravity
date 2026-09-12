import type { ReactElement } from "react";
import { updatePrefs, usePrefs } from "../../prefs";

/** Client-side notification behaviour. */
export default function NotificationSettings(): ReactElement {
  const prefs = usePrefs();

  return (
    <div className="settings-section">
      <div className="settings-row">
        <div className="settings-row-text">
          <label className="settings-row-label" htmlFor="settings-dock-badge">
            Dock badge
          </label>
          <div className="settings-row-help">
            Show what is waiting on you — unread messages and pending decisions — on the app icon in
            the Dock.
          </div>
        </div>
        <label className="toggle" htmlFor="settings-dock-badge" aria-label="Toggle dock badge">
          <input
            id="settings-dock-badge"
            type="checkbox"
            checked={prefs.dockBadge}
            onChange={(event) => {
              updatePrefs({ dockBadge: event.target.checked });
            }}
          />
          <span className="toggle-track" />
        </label>
      </div>

      <div className="settings-row">
        <div className="settings-row-text">
          <label className="settings-row-label" htmlFor="settings-decision-notifications">
            Urgent decisions
          </label>
          <div className="settings-row-help">
            Raise a system notification when a bot marks a decision urgent, or a deadline it named
            is within a day. Everything else stays a toast.
          </div>
        </div>
        <label
          className="toggle"
          htmlFor="settings-decision-notifications"
          aria-label="Toggle decision notifications"
        >
          <input
            id="settings-decision-notifications"
            type="checkbox"
            checked={prefs.decisionNotifications}
            onChange={(event) => {
              updatePrefs({ decisionNotifications: event.target.checked });
            }}
          />
          <span className="toggle-track" />
        </label>
      </div>
    </div>
  );
}
