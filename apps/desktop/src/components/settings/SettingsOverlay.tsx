import { useLayoutEffect, useRef } from "react";
import type { ReactElement } from "react";
import type { DaemonState } from "../../app/useDaemonState";
import type { AddToast } from "../../app/useToasts";
import type { DaemonApi } from "../../protocol/api";
import DevicesPanel from "../DevicesPanel";
import AboutSettings from "./AboutSettings";
import { SETTINGS_CATEGORIES } from "./categories";
import type { SettingsCategory } from "./categories";
import ConnectionSettings from "./ConnectionSettings";
import DiagnosticsSettings from "./DiagnosticsSettings";
import GeneralSettings from "./GeneralSettings";

interface CategoryPaneProps {
  readonly client: DaemonApi;
  readonly daemon: DaemonState;
  readonly category: SettingsCategory;
  readonly addToast: AddToast;
}

interface SettingsOverlayProps extends CategoryPaneProps {
  readonly onSelectCategory: (category: SettingsCategory) => void;
  readonly onClose: () => void;
}

function CategoryPane(props: CategoryPaneProps): ReactElement {
  const { client, daemon, category, addToast } = props;
  switch (category) {
    case "general": {
      return <GeneralSettings />;
    }
    case "connection": {
      return (
        <ConnectionSettings
          client={client}
          status={daemon.status}
          endpoint={daemon.endpoint}
          connected={daemon.connected}
          canControl={daemon.canControl}
          onChangeEndpoint={daemon.changeEndpoint}
          onToast={addToast}
        />
      );
    }
    case "devices": {
      return (
        <DevicesPanel
          client={client}
          connected={daemon.connected}
          canControl={daemon.canControl}
          onToast={addToast}
        />
      );
    }
    case "diagnostics": {
      return (
        <DiagnosticsSettings
          client={client}
          bots={daemon.bots}
          connected={daemon.connected}
          canControl={daemon.canControl}
          onToast={addToast}
        />
      );
    }
    case "about": {
      return (
        <AboutSettings
          endpoint={daemon.endpoint}
          daemonVersion={daemon.serverVersion}
          addToast={addToast}
        />
      );
    }
  }
}

/**
 * Full-window settings: a blurred backdrop with a near-fullscreen panel, a
 * category rail on the left and the selected category's pane on the right.
 * Floats above the current selection so closing it returns exactly there.
 */
export default function SettingsOverlay(props: SettingsOverlayProps): ReactElement {
  const { category, onSelectCategory, onClose } = props;
  const panelRef = useRef<HTMLDivElement | null>(null);

  useLayoutEffect(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const panel = panelRef.current;
    if (panel === null) {
      return undefined;
    }
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      const focusable = Array.from(
        panel.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (first === undefined || last === undefined) {
        event.preventDefault();
        return;
      }
      const active = document.activeElement;
      if (event.shiftKey && (active === first || !panel.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };
    panel.addEventListener("keydown", handleKeyDown);
    panel.querySelector<HTMLElement>("[aria-current='page']")?.focus();
    return () => {
      panel.removeEventListener("keydown", handleKeyDown);
      previousFocus?.focus();
    };
  }, [onClose]);

  return (
    <div
      className="settings-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        ref={panelRef}
        className="settings-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        tabIndex={-1}
      >
        <nav className="settings-rail" aria-label="Settings categories">
          <div className="settings-rail-title">Settings</div>
          {SETTINGS_CATEGORIES.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={`row ${entry.id === category ? "row-selected" : ""}`}
              aria-current={entry.id === category ? "page" : undefined}
              onClick={() => {
                onSelectCategory(entry.id);
              }}
            >
              <entry.icon
                className="settings-rail-icon"
                size={15}
                strokeWidth={1.75}
                aria-hidden="true"
              />
              <span className="row-name">{entry.label}</span>
            </button>
          ))}
        </nav>
        <div className="settings-pane">
          <div className="settings-pane-header">
            <h2 className="settings-pane-title">
              {SETTINGS_CATEGORIES.find((entry) => entry.id === category)?.label ?? ""}
            </h2>
            <button
              type="button"
              className="btn btn-small"
              aria-label="Close settings"
              onClick={onClose}
            >
              Close
            </button>
          </div>
          <div className="settings-pane-body">
            <CategoryPane {...props} />
          </div>
        </div>
      </div>
    </div>
  );
}
