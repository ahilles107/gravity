import { Activity, Info, Plug, Settings2, Smartphone } from "lucide-react";
import type { LucideIcon } from "lucide-react";

/** Categories in the settings overlay's left rail, in display order. */
export type SettingsCategory = "general" | "connection" | "devices" | "diagnostics" | "about";

export const SETTINGS_CATEGORIES: readonly {
  id: SettingsCategory;
  label: string;
  icon: LucideIcon;
}[] = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "connection", label: "Connection", icon: Plug },
  { id: "devices", label: "Devices", icon: Smartphone },
  { id: "diagnostics", label: "Diagnostics", icon: Activity },
  { id: "about", label: "About", icon: Info },
];
