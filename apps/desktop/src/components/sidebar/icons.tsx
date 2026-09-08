import type { ReactElement } from "react";

/** Line icons for the sidebar header; sized by the surrounding font-size. */

export function PlusIcon(): ReactElement {
  return (
    <svg viewBox="0 0 16 16" className="icon" aria-hidden="true">
      <path d="M8 3v10M3 8h10" />
    </svg>
  );
}

export function SearchIcon(): ReactElement {
  return (
    <svg viewBox="0 0 16 16" className="icon" aria-hidden="true">
      <circle cx="7" cy="7" r="4.25" />
      <path d="M10.2 10.2 13.5 13.5" />
    </svg>
  );
}

export function MoreIcon(): ReactElement {
  return (
    <svg viewBox="0 0 16 16" className="icon icon-dots" aria-hidden="true">
      <path d="M3.5 8h0M8 8h0M12.5 8h0" />
    </svg>
  );
}
