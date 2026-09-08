import { describe, expect, it } from "vitest";
import * as fx from "../../test/fixtures";
import { buildTrigger, EMPTY_TRIGGER_DRAFT, triggerSummary } from "./triggers";

const draft = EMPTY_TRIGGER_DRAFT;

describe("buildTrigger", () => {
  it("builds a cron trigger", () => {
    expect(buildTrigger({ ...draft, cronExpr: " 0 0 9 * * MON ", tz: " UTC " })).toEqual({
      kind: "cron",
      expr: "0 0 9 * * MON",
      tz: "UTC",
    });
  });

  it("rejects a cron trigger missing an expression or timezone", () => {
    expect(buildTrigger({ ...draft, cronExpr: "  " })).toBeNull();
    expect(buildTrigger({ ...draft, tz: "" })).toBeNull();
  });

  it("builds an interval trigger", () => {
    expect(buildTrigger({ ...draft, kind: "interval", intervalSeconds: "900" })).toEqual({
      kind: "interval",
      seconds: 900,
    });
  });

  it("rejects a non-positive or unparseable interval", () => {
    expect(buildTrigger({ ...draft, kind: "interval", intervalSeconds: "0" })).toBeNull();
    expect(buildTrigger({ ...draft, kind: "interval", intervalSeconds: "soon" })).toBeNull();
  });

  it("builds a signal trigger narrowed to one bot", () => {
    expect(
      buildTrigger({
        ...draft,
        kind: "signal",
        signalName: " deploy.finished ",
        fromBotId: "b1",
      }),
    ).toEqual({ kind: "signal", name: "deploy.finished", from_bot_id: "b1" });
  });

  it("omits the source when the signal may come from any bot", () => {
    expect(
      buildTrigger({
        ...draft,
        kind: "signal",
        signalName: "deploy.finished",
        fromBotId: "",
      }),
    ).toEqual({ kind: "signal", name: "deploy.finished" });
  });

  it("rejects a signal trigger without a name", () => {
    expect(buildTrigger({ ...draft, kind: "signal", signalName: "  " })).toBeNull();
  });
});

describe("triggerSummary", () => {
  const bots = [fx.bot({ id: "b1", name: "alice" })];

  it("describes each trigger kind", () => {
    expect(triggerSummary({ kind: "cron", expr: "0 9 * * *", tz: "UTC" }, bots)).toBe(
      "cron 0 9 * * * (UTC)",
    );
    expect(triggerSummary({ kind: "interval", seconds: 60 }, bots)).toBe("every 60s");
    expect(
      triggerSummary({ kind: "signal", name: "deploy.finished", from_bot_id: "b1" }, bots),
    ).toBe("on signal deploy.finished from alice");
    expect(triggerSummary({ kind: "signal", name: "deploy.finished" }, bots)).toBe(
      "on signal deploy.finished",
    );
  });

  it("falls back to the bot id when the bot is unknown", () => {
    expect(triggerSummary({ kind: "signal", name: "ci.failed", from_bot_id: "ghost" }, bots)).toBe(
      "on signal ci.failed from ghost",
    );
  });
});
