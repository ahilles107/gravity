import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DragDropEvent } from "@tauri-apps/api/window";

interface TerminalFileDropOptions {
  readonly element: HTMLElement;
  readonly canDrop: () => boolean;
  readonly onDrop: (input: string) => void;
  readonly onError: (error: unknown) => void;
}

function quotePath(path: string): string {
  return `'${path.replaceAll("'", `'\\''`)}'`;
}

export function formatDroppedPaths(paths: readonly string[]): string {
  return `${paths.map(quotePath).join(" ")} `;
}

function isOverElement(event: DragDropEvent, element: HTMLElement, scaleFactor: number): boolean {
  if (event.type === "leave") {
    return false;
  }
  const position = event.position.toLogical(scaleFactor);
  const bounds = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  const hitTarget = document.elementFromPoint(position.x, position.y);
  return (
    style.display !== "none" &&
    style.visibility !== "hidden" &&
    bounds.width > 0 &&
    bounds.height > 0 &&
    position.x >= bounds.left &&
    position.x <= bounds.right &&
    position.y >= bounds.top &&
    position.y <= bounds.bottom &&
    hitTarget !== null &&
    element.contains(hitTarget)
  );
}

export function listenForTerminalFileDrops(options: TerminalFileDropOptions): () => void {
  if (!isTauri()) {
    return () => undefined;
  }

  let disposed = false;
  let unlisten: (() => void) | undefined;

  const register = async (): Promise<void> => {
    const appWindow = getCurrentWindow();
    const handleDrop = async (payload: DragDropEvent): Promise<void> => {
      if (payload.type !== "drop" || payload.paths.length === 0 || !options.canDrop()) {
        return;
      }
      try {
        const scaleFactor = await appWindow.scaleFactor();
        if (!disposed && isOverElement(payload, options.element, scaleFactor)) {
          options.onDrop(formatDroppedPaths(payload.paths));
        }
      } catch (error) {
        if (!disposed) {
          options.onError(error);
        }
      }
    };
    const removeListener = await appWindow.onDragDropEvent(({ payload }) => {
      void handleDrop(payload);
    });
    if (disposed) {
      removeListener();
      return;
    }
    unlisten = removeListener;
  };

  void register().catch((error: unknown) => {
    if (!disposed) {
      options.onError(error);
    }
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}
