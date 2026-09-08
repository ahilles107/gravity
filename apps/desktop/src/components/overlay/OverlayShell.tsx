import type { ReactElement, ReactNode } from "react";

interface OverlayShellProps {
  readonly label: string;
  readonly onClose: () => void;
  readonly children: ReactNode;
}

/** Backdrop + centred panel shared by the command palette and search overlay. */
export default function OverlayShell({
  label,
  onClose,
  children,
}: OverlayShellProps): ReactElement {
  return (
    <div
      className="overlay-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div className="overlay-panel" role="dialog" aria-modal="true" aria-label={label}>
        {children}
      </div>
    </div>
  );
}
