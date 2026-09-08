import { useState } from "react";
import type { MouseEvent, ReactElement } from "react";
import ConfirmDialog from "../overlay/ConfirmDialog";
import RowContextMenu from "./RowContextMenu";

/** Where the menu opened: at the pointer, or hung off a trigger's right edge. */
interface MenuAnchor {
  readonly x: number;
  readonly y: number;
  readonly align: "start" | "end";
}

interface RowMenuItem {
  readonly label: string;
  readonly onSelect: () => void;
}

/** The destructive entry, which is confirmed before it runs. */
interface RowMenuDeletion {
  readonly label: string;
  readonly title: string;
  readonly body: string;
  readonly confirmLabel: string;
  readonly onConfirm: () => void;
}

interface RowMenuOptions {
  readonly items: readonly RowMenuItem[];
  /** Null when the connection may not delete: the entry is left out entirely. */
  readonly deletion: RowMenuDeletion | null;
}

export interface RowMenuApi {
  readonly onContextMenu: (event: MouseEvent<HTMLElement>) => void;
  /** Opens the same menu under a trigger button, for a row's ⋯ affordance. */
  readonly onOpenFrom: (event: MouseEvent<HTMLElement>) => void;
  /** The menu and its confirmation, rendered next to the row it belongs to. */
  readonly overlays: ReactElement;
}

/**
 * Right-click menu plumbing shared by every sidebar row: where the menu opened,
 * and the confirmation guarding its one destructive entry.
 */
export function useRowMenu({ items, deletion }: RowMenuOptions): RowMenuApi {
  const [menuAt, setMenuAt] = useState<MenuAnchor | null>(null);
  const [confirming, setConfirming] = useState(false);

  const entries = [
    ...items,
    ...(deletion === null
      ? []
      : [
          {
            label: deletion.label,
            danger: true,
            onSelect: (): void => {
              setConfirming(true);
            },
          },
        ]),
  ];

  return {
    onContextMenu: (event) => {
      event.preventDefault();
      setMenuAt({ x: event.clientX, y: event.clientY, align: "start" });
    },
    onOpenFrom: (event) => {
      const rect = event.currentTarget.getBoundingClientRect();
      setMenuAt({ x: rect.right, y: rect.bottom + 4, align: "end" });
    },
    overlays: (
      <>
        {menuAt === null ? null : (
          <RowContextMenu
            x={menuAt.x}
            y={menuAt.y}
            align={menuAt.align}
            items={entries}
            onClose={() => {
              setMenuAt(null);
            }}
          />
        )}
        {confirming && deletion !== null ? (
          <ConfirmDialog
            title={deletion.title}
            body={deletion.body}
            confirmLabel={deletion.confirmLabel}
            onConfirm={() => {
              setConfirming(false);
              deletion.onConfirm();
            }}
            onCancel={() => {
              setConfirming(false);
            }}
          />
        ) : null}
      </>
    ),
  };
}
