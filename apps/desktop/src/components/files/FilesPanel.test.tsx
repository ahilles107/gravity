import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";
import type { FileBody } from "../../protocol/chat";
import { FakeDaemon } from "../../test/fakeDaemon";
import * as fx from "../../test/fixtures";
import FilesPanel from "./FilesPanel";

function daemon(files: Readonly<Record<string, FileBody>>): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_artifacts", () => ({
      type: "artifacts",
      req_id: "1",
      project_id: "p1",
      artifacts: [
        {
          path: "/p/artifacts/report.md",
          rel: "report.md",
          name: "report.md",
          size: 2048,
          modified: null,
          mime: "text/markdown",
          title: "Weekly report",
        },
        {
          path: "/p/artifacts/app.ts",
          rel: "src/app.ts",
          name: "app.ts",
          size: 40,
          mime: "text/x-code",
        },
      ],
    }))
    .onRequest("read_file", (body) => {
      const file = body.type === "read_file" ? files[body.path] : undefined;
      if (file === undefined) {
        throw new Error("file not found");
      }
      return { type: "file", req_id: "2", file };
    });
}

function Harness({ client }: { readonly client: FakeDaemon }): ReactElement {
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <FilesPanel
      client={client}
      bot={fx.bot()}
      connected
      selected={selected}
      onSelect={setSelected}
    />
  );
}

describe("FilesPanel", () => {
  it("lists artifacts and previews markdown and code", async () => {
    render(
      <Harness
        client={daemon({
          "/p/artifacts/report.md": {
            name: "report.md",
            mime: "text/markdown",
            text: "# Weekly\n\nAll **green**.",
            truncated: false,
          },
          "/p/artifacts/app.ts": {
            name: "app.ts",
            mime: "text/x-code",
            text: "const x = 1;",
            truncated: true,
          },
        })}
      />,
    );
    expect(await screen.findByText("Weekly report")).toBeInTheDocument();
    expect(screen.getByText("report.md · 2 KB")).toBeInTheDocument();
    expect(screen.getByText("src/app.ts · 40 B")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Weekly report/ }));
    expect(await screen.findByText("green")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "← Files" }));
    await userEvent.click(await screen.findByRole("button", { name: /app.ts/ }));
    expect(await screen.findByText("const")).toHaveClass("tok-keyword");
    expect(screen.getByText(/first 16 MB/)).toBeInTheDocument();
  });

  it("shows images, unsupported files and refusals", async () => {
    const client = daemon({
      "/shot.png": { name: "shot.png", mime: "image/png", base64: "AAAA", truncated: false },
      "/a.bin": {
        name: "a.bin",
        mime: "application/octet-stream",
        base64: "AA==",
        truncated: false,
      },
    });
    const { rerender } = render(
      <FilesPanel
        client={client}
        bot={fx.bot()}
        connected
        selected="/shot.png"
        onSelect={() => {}}
      />,
    );
    expect(await screen.findByRole("img", { name: "shot.png" })).toHaveAttribute(
      "src",
      "data:image/png;base64,AAAA",
    );
    rerender(
      <FilesPanel client={client} bot={fx.bot()} connected selected="/a.bin" onSelect={() => {}} />,
    );
    expect(await screen.findByText(/No preview for this kind of file/)).toBeInTheDocument();
    rerender(
      <FilesPanel
        client={client}
        bot={fx.bot()}
        connected
        selected="/etc/hosts"
        onSelect={() => {}}
      />,
    );
    expect(await screen.findByText("file not found")).toBeInTheDocument();
  });
});
