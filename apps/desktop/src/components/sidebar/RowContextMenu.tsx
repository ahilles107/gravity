import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactElement, ReactNode } from "react";

interface MenuItem {
  readonly label: string;
  readonly icon?: ReactNode;
  readonly danger?: boolean;
  readonly onSelect: () => void;
}

interface RowContextMenuProps {
  readonly x: number;
  readonly y: number;
  /** "end" hangs the menu's right edge on x, for a trigger button's ⋯ affordance. */
  readonly align?: "start" | "end";
  readonly items: readonly MenuItem[];
  readonly onClose: () => void;
}

interface Point {
  readonly x: number;
  readonly y: number;
}

/** Keeps the menu off the window edges when the anchor sits near one. */
const VIEWPORT_MARGIN = 8;

function clamp(value: number, max: number): number {
  return Math.max(VIEWPORT_MARGIN, Math.min(value, max));
}

/** Right-click menu anchored at the pointer, dismissed by Escape or any press outside it. */
export default function RowContextMenu({
  x,
  y,
  align = "start",
  items,
  onClose,
}: RowContextMenuProps): ReactElement {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [placement, setPlacement] = useState<Point | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  // The menu is sized by its longest label, so it can only be aligned to the
  // anchor once it has been laid out; this runs before paint, so it never shows
  // at the unaligned spot.
  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menu === null) {
      return;
    }
    const { width, height } = menu.getBoundingClientRect();
    setPlacement({
      x: clamp(align === "end" ? x - width : x, window.innerWidth - width - VIEWPORT_MARGIN),
      y: clamp(y, window.innerHeight - height - VIEWPORT_MARGIN),
    });
  }, [align, x, y]);

  return (
    <div
      className="menu-backdrop"
      role="presentation"
      onMouseDown={onClose}
      onContextMenu={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <div
        ref={menuRef}
        className="context-menu"
        role="menu"
        style={{
          left: placement === null ? x : placement.x,
          top: placement === null ? y : placement.y,
          visibility: placement === null ? "hidden" : "visible",
        }}
      >
        {items.map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            className={`context-menu-item ${item.danger === true ? "context-menu-danger" : ""}`}
            onMouseDown={(event) => {
              event.stopPropagation();
            }}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            {item.icon === undefined ? null : (
              <span className="context-menu-icon">{item.icon}</span>
            )}
            {item.label}
          </button>
        ))}
      </div>
    </div>
  );
}
