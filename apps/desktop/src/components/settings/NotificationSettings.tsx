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
            Show the total unread count on the app icon in the Dock.
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
    </div>
  );
}
