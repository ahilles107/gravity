import { render } from "@testing-library/react";
import { vi } from "vitest";
import type { Selection } from "../app/selection";
import type { SettingsCategory } from "../components/settings/categories";
import Sidebar from "../components/Sidebar";
import * as fx from "./fixtures";

/** Mocks are declared up front so their call signatures survive the spread. */
function mocks() {
  return {
    onSelect: vi.fn<(selection: Selection) => void>(),
    onOpenSearch: vi.fn<() => void>(),
    onCreateProject: vi.fn<(name: string) => Promise<void>>(() => Promise.resolve()),
    onCreateBot: vi.fn<(projectId: string) => Promise<void>>(() => Promise.resolve()),
    onDeleteBot: vi.fn<(botId: string) => Promise<void>>(() => Promise.resolve()),
    onDeleteProject: vi.fn<(projectId: string) => Promise<void>>(() => Promise.resolve()),
    onOpenSettings: vi.fn<(category?: SettingsCategory) => void>(),
  };
}

type SidebarSpies = ReturnType<typeof mocks>;

/** Renders the sidebar with workable defaults, returning the spies it was given. */
export function renderSidebar(over: Partial<Parameters<typeof Sidebar>[0]> = {}): SidebarSpies {
  const spies = mocks();
  const props = {
    status: "connected" as const,
    endpoint: { host: "127.0.0.1", port: 7777 },
    projects: [fx.project()],
    bots: [fx.bot()],
    unreadBots: {},
    failedByBot: new Map<string, number>(),
    nextRun: {},
    activityByBot: {},
    selection: { kind: "none" } as const,
    canControl: true,
    ...spies,
    ...over,
  };
  render(<Sidebar {...props} />);
  return spies;
}
