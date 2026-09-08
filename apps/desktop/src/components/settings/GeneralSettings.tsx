import type { ReactElement } from "react";
import AppearanceSettings from "./AppearanceSettings";
import NotificationSettings from "./NotificationSettings";
import ShortcutSettings from "./ShortcutSettings";

/** General preferences: terminal appearance, notifications and global shortcuts. */
export default function GeneralSettings(): ReactElement {
  return (
    <>
      <h3 className="settings-subhead">Appearance</h3>
      <AppearanceSettings />
      <h3 className="settings-subhead">Notifications</h3>
      <NotificationSettings />
      <h3 className="settings-subhead">Shortcuts</h3>
      <ShortcutSettings />
    </>
  );
}
