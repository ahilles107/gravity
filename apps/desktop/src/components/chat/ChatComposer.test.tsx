import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ChatComposer, { withAttachments } from "./ChatComposer";

describe("ChatComposer", () => {
  it("attaches files and sends their paths with the message", async () => {
    const onSend = vi.fn<(text: string) => Promise<void>>().mockResolvedValue(undefined);
    const onAttach = vi
      .fn<(file: File) => Promise<string>>()
      .mockResolvedValue("/p/artifacts/uploads/spec.pdf");
    render(
      <ChatComposer
        disabledReason={null}
        placeholder="Message"
        onSend={onSend}
        onAttach={onAttach}
      />,
    );

    await userEvent.upload(
      screen.getByLabelText("Files to attach"),
      new File(["%PDF"], "spec.pdf", { type: "application/pdf" }),
    );
    expect(await screen.findByText("spec.pdf")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByText(/uploading/)).not.toBeInTheDocument();
    });
    await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "read this{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      "read this\n\nAttached files:\n- /p/artifacts/uploads/spec.pdf",
    );
    expect(screen.queryByText("spec.pdf")).not.toBeInTheDocument();
  });

  it("shows a failed upload and lets it be removed", async () => {
    const onAttach = vi
      .fn<(file: File) => Promise<string>>()
      .mockRejectedValue(new Error("too big"));
    render(
      <ChatComposer
        disabledReason={null}
        placeholder="Message"
        onSend={vi.fn<(text: string) => Promise<void>>()}
        onAttach={onAttach}
      />,
    );
    await userEvent.upload(screen.getByLabelText("Files to attach"), new File(["x"], "huge.bin"));
    expect(await screen.findByText(/failed/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remove huge.bin" }));
    expect(screen.queryByText(/huge.bin/)).not.toBeInTheDocument();
  });

  it("says where a slash command runs", async () => {
    render(
      <ChatComposer
        disabledReason={null}
        placeholder="Message"
        onSend={vi.fn<(text: string) => Promise<void>>()}
        slashHint="Runs in the terminal"
      />,
    );
    await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "/compact");
    expect(screen.getByText("Runs in the terminal")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Attach files" })).not.toBeInTheDocument();
  });

  it("lists attachments after the text, or alone", () => {
    const done = [{ key: 1, name: "a", path: "/a" }];
    expect(withAttachments("", done)).toBe("Attached files:\n- /a");
    expect(withAttachments("hi", [])).toBe("hi");
  });
});
