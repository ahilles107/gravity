import type { ReactElement, ReactNode } from "react";

interface PanelHeaderProps {
  readonly title: string;
  readonly children?: ReactNode;
}

/** Panel title row with an optional right-aligned action area. */
export default function PanelHeader({ title, children }: PanelHeaderProps): ReactElement {
  return (
    <div className="panel-header">
      <h3 className="panel-title">{title}</h3>
      {children}
    </div>
  );
}
