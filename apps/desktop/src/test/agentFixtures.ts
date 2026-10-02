import type {
  AgentBot,
  AgentConversation,
  AgentMessage,
  BrowserAction,
  BrowserTabsPush,
} from "../protocol/agents";
import { FakeDaemon } from "./fakeDaemon";
import * as fx from "./fixtures";

const AT = "2025-01-15T10:00:00Z";

export const lead: AgentBot = { id: "b1", name: "lead", avatar: "icon:comet", deleted: false };
export const windev: AgentBot = {
  id: "b2",
  name: "windev",
  avatar: "icon:frost",
  machine: "win-pc",
  deleted: false,
};
export const qa: AgentBot = { id: "b3", name: "qa", avatar: "color:#c084fc", deleted: false };

export function agentMessage(over: Partial<AgentMessage> = {}): AgentMessage {
  return {
    id: "m1",
    num: 1,
    from_bot_id: lead.id,
    to_bot_id: windev.id,
    kind: "task",
    body: "Port the updater to Windows.",
    task: { id: "t1", state: "open" },
    created_at: AT,
    ...over,
  };
}

export const thread: readonly AgentMessage[] = [
  agentMessage(),
  agentMessage({
    id: "m2",
    num: 2,
    from_bot_id: windev.id,
    to_bot_id: lead.id,
    kind: "reply",
    body: "Which signing certificate should the **MSI** use?",
    task: null,
    created_at: "2025-01-15T10:05:00Z",
  }),
  agentMessage({
    id: "m3",
    num: 3,
    kind: "reply",
    body: "The release one, in `certs/release.pfx`.",
    task: null,
    created_at: "2025-01-15T10:06:00Z",
  }),
  agentMessage({
    id: "m4",
    num: 4,
    from_bot_id: windev.id,
    to_bot_id: lead.id,
    kind: "done",
    body: "Ported. The updater builds and its tests pass on Windows.",
    task: null,
    created_at: "2025-01-15T11:20:00Z",
  }),
];

export const conversations: readonly AgentConversation[] = [
  {
    bot_ids: [lead.id, windev.id],
    message_count: 4,
    last_at: "2025-01-15T11:20:00Z",
    last: thread[3] ?? null,
  },
  {
    bot_ids: [qa.id, lead.id],
    message_count: 1,
    last_at: "2025-01-15T09:00:00Z",
    last: agentMessage({
      id: "m9",
      num: 0,
      from_bot_id: lead.id,
      to_bot_id: qa.id,
      kind: "note",
      body: "Heads up: the installer changes land today.",
      task: null,
      created_at: "2025-01-15T09:00:00Z",
    }),
  },
];

export const projectBots = [
  fx.bot({ id: lead.id, name: lead.name, avatar: lead.avatar }),
  fx.bot({ id: windev.id, name: windev.name, avatar: windev.avatar }),
  fx.bot({ id: qa.id, name: qa.name, avatar: qa.avatar }),
];

export const browserTabs: BrowserTabsPush = {
  type: "browser_tabs",
  bot_id: "b1",
  open: true,
  active: "tab-1",
  following: true,
  tabs: [
    { id: "tab-1", title: "Pricing — Example", url: "https://example.com/pricing" },
    { id: "tab-2", title: "Docs", url: "https://docs.example.com/" },
  ],
};

export const browserActivity: readonly BrowserAction[] = [
  {
    turn_id: "turn-2",
    step_id: "s3",
    at: "2025-01-15T10:10:00Z",
    browser: "owners_chrome",
    title: "Opened mail.example.com",
    subtitle: "https://mail.example.com/",
    status: "ok",
    trigger: { kind: "owner", text: "Check whether the invoice arrived", via: "chat" },
  },
  {
    turn_id: "turn-1",
    step_id: "s2",
    at: "2025-01-15T10:00:00Z",
    browser: "own",
    title: "Clicked Plans tab",
    status: "ok",
    trigger: {
      kind: "bus",
      from: "lead",
      msg_kind: "task",
      num: 7,
      text: "Compare the pricing tiers",
    },
  },
  {
    turn_id: "turn-1",
    step_id: "s1",
    at: "2025-01-15T10:00:00Z",
    browser: "own",
    title: "Opened example.com",
    subtitle: "https://example.com/pricing",
    status: "ok",
    trigger: {
      kind: "bus",
      from: "lead",
      msg_kind: "task",
      num: 7,
      text: "Compare the pricing tiers",
    },
  },
];

const facts =
  "# Facts\n\n- The release certificate lives in `certs/release.pfx`.\n- CI runs on the `win-2022` image.";

/** A daemon serving bot browsers, memory and conversations from the fixtures above. */
export function agentsDaemon(): FakeDaemon {
  const client = new FakeDaemon()
    .onRequest("watch_browser", () => ({ type: "ok", req_id: "1" }))
    .onRequest("unwatch_browser", () => ({ type: "ok", req_id: "1" }))
    .onRequest("list_browser_activity", () => ({
      type: "browser_activity",
      req_id: "1",
      bot_id: "b1",
      activity: browserActivity,
    }))
    .onRequest("read_file", () => ({
      type: "file",
      req_id: "1",
      file: { name: "FACTS.md", mime: "text/markdown", text: facts, truncated: false },
    }))
    .onRequest("list_agent_conversations", () => ({
      type: "agent_conversations",
      req_id: "1",
      project_id: "p1",
      conversations,
      bots: [lead, windev, qa],
    }))
    .onRequest("list_agent_conversation", (body) => ({
      type: "agent_conversation",
      req_id: "1",
      project_id: "p1",
      bot_ids: [lead.id, windev.id],
      messages: body.type === "list_agent_conversation" && body.before !== undefined ? [] : thread,
      has_more: false,
      bots: [lead, windev],
    }));
  client.capabilities = [...client.capabilities, "chat", "bot_browser", "agent_conversations"];
  return client;
}
