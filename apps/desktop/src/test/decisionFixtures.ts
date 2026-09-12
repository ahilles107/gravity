import type { Decision, DecisionComment, PendingCounts, Tag } from "../protocol/decisions";

const AT = "2024-05-01T09:00:00.000Z";

export function decision(over: Partial<Decision> = {}): Decision {
  return {
    id: "d1",
    project_id: "p1",
    kind: "decision",
    title: "Waive rule 3 and start the ads today?",
    body: "The 17+ listing is live. Spend starts at $40/day.",
    options: [
      { key: "start", label: "Start today", description: "Spend begins tonight." },
      { key: "hold", label: "Hold until the listing settles" },
    ],
    recommendation: "start",
    raised_by: { bot_id: "b1", name: "auction", avatar: "icon:orbit" },
    origin_chain: "",
    priority: "normal",
    state: "open",
    tags: ["spend", "apple-ads"],
    comment_count: 0,
    created_at: AT,
    ...over,
  };
}

export function decisionComment(over: Partial<DecisionComment> = {}): DecisionComment {
  return {
    id: "c1",
    decision_id: "d1",
    author_kind: "user",
    author_name: "you",
    body: "Tell me more about the Backblaze caps first.",
    created_at: AT,
    ...over,
  };
}

export function tag(over: Partial<Tag> = {}): Tag {
  return {
    id: "t1",
    name: "spend",
    description: "money leaving the account",
    color: "",
    created_by: "b1",
    uses: { p1: 3 },
    open_uses: 1,
    ...over,
  };
}

export function pendingCounts(over: Partial<PendingCounts> = {}): PendingCounts {
  return { by_project: { p1: 1 }, total: 1, urgent: 0, due_soon: 0, ...over };
}
