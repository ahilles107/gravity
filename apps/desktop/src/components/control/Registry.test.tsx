import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef, useState } from "react";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import type { Decision, Tag } from "../../protocol/decisions";
import * as dfx from "../../test/decisionFixtures";
import { NO_FILTER } from "./decisions";
import type { RegistryFilter } from "./decisions";
import Registry from "./Registry";

const RULED = dfx.decision({
  id: "d1",
  title: "Waive rule 3 and start the ads today?",
  published_at: "2026-09-20T10:00:00Z",
  state: "settled",
  tags: ["spend"],
  ruling: {
    option: "start",
    text: "Start today.",
    answered_at: "2026-09-20T10:00:00Z",
    answered_by: "owner",
  },
});

const NIGHTLY = dfx.decision({
  id: "d2",
  title: "Drop the nightly Backblaze run?",
  published_at: "2026-09-05T10:00:00Z",
  state: "settled",
  project_id: "p2",
  tags: ["backups"],
  raised_by: { bot_id: "b2", name: "chief", avatar: "" },
  options: [],
  ruling: {
    text: "Keep the nightly.",
    answered_at: "2026-09-05T10:00:00Z",
    answered_by: "owner",
  },
});

const CALLED_OFF = dfx.decision({
  id: "d3",
  title: "Pin Dozzle?",
  published_at: "2026-08-28T10:00:00Z",
  state: "withdrawn",
  withdrawn_reason: "the image was archived",
  tags: ["spend"],
});

const ROWS: readonly Decision[] = [RULED, NIGHTLY, CALLED_OFF];

const TAGS: readonly Tag[] = [
  dfx.tag({ id: "t1", name: "spend", uses: { p1: 2 } }),
  dfx.tag({ id: "t2", name: "backups", uses: { p1: 1 } }),
  dfx.tag({ id: "t3", name: "gone", retired_at: "2026-08-01T00:00:00Z" }),
];

const PROJECT_NAMES: Record<string, string> = { p1: "Acme", p2: "Zephyr" };

function Harness({ onOpen }: { readonly onOpen: (id: string) => void }): ReactElement {
  const [filter, setFilter] = useState<RegistryFilter>(NO_FILTER);
  const narrow = (over: Partial<RegistryFilter>): void => {
    setFilter((prev) => ({ ...prev, ...over }));
  };
  return (
    <div className="control-center">
      <Registry
        rows={ROWS}
        settledCount={2}
        tags={TAGS}
        filter={filter}
        onQuery={(query) => narrow({ query })}
        onTagFilter={(tagFilter) => narrow({ tagFilter })}
        onProjectFilter={(projectFilter) => narrow({ projectFilter, botFilter: undefined })}
        onBotFilter={(botFilter) => narrow({ botFilter })}
        searchRef={createRef<HTMLInputElement>()}
        onOpen={onOpen}
        projectName={(projectId) => PROJECT_NAMES[projectId] ?? "—"}
      />
    </div>
  );
}

function renderRegistry() {
  const onOpen = vi.fn<(id: string) => void>();
  render(<Harness onOpen={onOpen} />);
  return { onOpen, user: userEvent.setup() };
}

describe("Registry", () => {
  it("files each ruling under the month it was given, in the owner's words", () => {
    renderRegistry();
    expect(screen.getByText("September 2026")).toBeInTheDocument();
    expect(screen.getByText("August 2026")).toBeInTheDocument();
    expect(screen.getByText("“Start today.”")).toBeInTheDocument();
    expect(screen.getByText("start")).toBeInTheDocument();
    expect(screen.getByText("2 of 2 rulings")).toBeInTheDocument();
  });

  // A registry that quietly drops what was called off is not the whole record.
  it("keeps a withdrawn decision and says so", () => {
    renderRegistry();
    expect(screen.getByText("Withdrawn — “the image was archived”")).toBeInTheDocument();
  });

  it("narrows by the bot that asked", async () => {
    const { user } = renderRegistry();
    await user.type(screen.getByLabelText("Search rulings"), "chief");
    expect(screen.getByText("“Keep the nightly.”")).toBeInTheDocument();
    expect(screen.queryByText("“Start today.”")).not.toBeInTheDocument();
    expect(screen.getByText("1 of 2 rulings")).toBeInTheDocument();
  });

  it("filters by a tag chip and lets it go again", async () => {
    const { user } = renderRegistry();
    expect(screen.queryByRole("button", { name: /gone/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "backups 1" }));
    expect(screen.getByText("“Keep the nightly.”")).toBeInTheDocument();
    expect(screen.queryByText("“Start today.”")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "backups 1" }));
    expect(screen.getByText("“Start today.”")).toBeInTheDocument();
  });

  // Chips for bots the project has none of are worse than no chips at all.
  it("filters by project and renarrows the bot chips to it", async () => {
    const { user } = renderRegistry();
    await user.click(screen.getByRole("button", { name: "Zephyr 1" }));
    expect(screen.getByText("“Keep the nightly.”")).toBeInTheDocument();
    expect(screen.queryByText("“Start today.”")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "auction 2" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "chief 1" })).toBeInTheDocument();
  });

  // A bot chosen under the old project would leave the ledger empty with no
  // chip on screen saying why.
  it("drops the bot chosen under another project", async () => {
    const { user } = renderRegistry();
    await user.click(screen.getByRole("button", { name: "chief 1" }));
    await user.click(screen.getByRole("button", { name: "Acme 2" }));
    expect(screen.queryByRole("button", { name: "chief 1" })).not.toBeInTheDocument();
    expect(screen.getByText("“Start today.”")).toBeInTheDocument();
    expect(screen.getByText("Withdrawn — “the image was archived”")).toBeInTheDocument();
  });

  it("filters by the bot that raised the ruling", async () => {
    const { user } = renderRegistry();
    await user.click(screen.getByRole("button", { name: "chief 1" }));
    expect(screen.getByText("“Keep the nightly.”")).toBeInTheDocument();
    expect(screen.queryByText("“Start today.”")).not.toBeInTheDocument();
    expect(screen.getByText("1 of 2 rulings")).toBeInTheDocument();
  });

  it("opens the row that was clicked", async () => {
    const { onOpen, user } = renderRegistry();
    await user.click(screen.getByText("“Start today.”"));
    expect(onOpen).toHaveBeenCalledWith("d1");
  });

  it("says so when nothing matches rather than showing a blank ledger", async () => {
    const { user } = renderRegistry();
    await user.type(screen.getByLabelText("Search rulings"), "backblaze caps");
    expect(screen.getByText("No rulings match.")).toBeInTheDocument();
    expect(screen.queryByText("September 2026")).not.toBeInTheDocument();
  });
});
