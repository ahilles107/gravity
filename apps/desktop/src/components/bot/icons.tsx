import { PanelRightClose, PanelRightOpen } from "lucide-react";
import type { ReactElement } from "react";

interface InfoPanelIconProps {
  readonly collapsed: boolean;
}

/** Lucide panel control matching the action available in the current state. */
export function InfoPanelIcon({ collapsed }: InfoPanelIconProps): ReactElement {
  const Icon = collapsed ? PanelRightOpen : PanelRightClose;
  return <Icon size={18} strokeWidth={1.75} aria-hidden="true" />;
}
