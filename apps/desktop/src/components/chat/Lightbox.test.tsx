import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import Lightbox from "./Lightbox";

function daemon(): FakeDaemon {
  return new FakeDaemon().onRequest("get_chat_image", (body) => ({
    type: "file",
    req_id: "1",
    file: {
      name: "x",
      mime: "image/png",
      base64: body.type === "get_chat_image" ? btoa(body.image_id) : "",
      truncated: false,
    },
  }));
}

const images = [
  { id: "wide", mime: "image/png" },
  { id: "tall", mime: "image/png" },
];

describe("Lightbox", () => {
  it("pages through a step's images and switches between fitted and actual size", async () => {
    const onClose = vi.fn<() => void>();
    render(<Lightbox client={daemon()} botId="b1" images={images} start={0} onClose={onClose} />);
    expect(screen.getByText("1 of 2")).toBeInTheDocument();
    const shown = await screen.findByRole("button", { name: "Show at actual size" });
    expect(shown.querySelector("img")).toHaveAttribute(
      "src",
      `data:image/png;base64,${btoa("wide")}`,
    );

    await userEvent.click(shown);
    expect(screen.getByText(/Actual size/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Fit to the window" }));
    expect(screen.getByText(/Fitted/)).toBeInTheDocument();

    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByText("2 of 2")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Next image" }));
    expect(screen.getByText("1 of 2")).toBeInTheDocument();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByText("2 of 2")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes from the backdrop but not from the image", async () => {
    const onClose = vi.fn<() => void>();
    render(
      <Lightbox
        client={daemon()}
        botId="b1"
        images={images.slice(0, 1)}
        start={0}
        onClose={onClose}
      />,
    );
    const image = await screen.findByRole("button", { name: "Show at actual size" });
    fireEvent.mouseDown(image);
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.mouseDown(screen.getByRole("dialog", { name: "Image" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "Next image" })).not.toBeInTheDocument();
  });
});
