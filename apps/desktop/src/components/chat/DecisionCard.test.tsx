import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Decision } from "../../protocol/decisions";
import { decision } from "../../test/decisionFixtures";
import { FakeDaemon } from "../../test/fakeDaemon";
import DecisionCard from "./DecisionCard";

function daemon(current: Decision): FakeDaemon {
  const client = new FakeDaemon()
    .onRequest("get_decision", () => ({ type: "decision", req_id: "1", decision: current }))
    .onRequest("publish_decisions", () => ({ type: "publish_result", req_id: "2", results: [] }));
  client.grants = ["read", "control", "approve"];
  return client;
}

describe("DecisionCard", () => {
  it("answers and publishes a decision from the chat", async () => {
    const client = daemon(decision());
    const onOpenDecision = vi.fn<(id: string) => void>();
    render(
      <DecisionCard
        client={client}
        decisionId="d1"
        title="Start the ads?"
        connected
        onOpenDecision={onOpenDecision}
      />,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Start today" }));
    expect(client.requests.find((r) => r.body.type === "publish_decisions")?.body).toEqual({
      type: "publish_decisions",
      items: [{ decision_id: "d1", ruling_text: "Start today", ruling_option: "start" }],
    });

    await userEvent.type(
      screen.getByRole("textbox", { name: "Another answer" }),
      "Wait a week{Enter}",
    );
    expect(client.requests.map((r) => r.body)).toContainEqual({
      type: "publish_decisions",
      items: [{ decision_id: "d1", ruling_text: "Wait a week" }],
    });

    await userEvent.click(screen.getByRole("button", { name: "Open in the Control Center" }));
    expect(onOpenDecision).toHaveBeenCalledWith("d1");
  });

  it("shows the ruling and no answers once settled", async () => {
    const client = daemon(decision());
    render(<DecisionCard client={client} decisionId="d1" title="Start the ads?" connected />);
    await screen.findByText(/Waiting on you/);
    act(() => {
      client.emit("decision_update", {
        type: "decision_update",
        decision: decision({
          state: "settled",
          ruling: { text: "Start them", answered_at: "2025-01-15T10:00:00Z", answered_by: "owner" },
        }),
      });
    });
    expect(await screen.findByText("You answered: Start them")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Another answer" })).not.toBeInTheDocument();
  });

  it("offers no answers without the approve grant", async () => {
    const client = daemon(decision());
    client.grants = ["read", "control"];
    render(<DecisionCard client={client} decisionId="d1" title="Start the ads?" connected />);
    await screen.findByText(/Waiting on you/);
    expect(screen.queryByRole("textbox", { name: "Another answer" })).not.toBeInTheDocument();
  });
});
