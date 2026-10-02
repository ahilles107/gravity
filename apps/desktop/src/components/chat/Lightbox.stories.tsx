import type { Story } from "@ladle/react";
import { FakeDaemon } from "../../test/fakeDaemon";
import Lightbox from "./Lightbox";

/** A wide screenshot-shaped image, drawn so the story needs no fixture file. */
function wideImage(): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="900"><rect width="1600" height="900" fill="#1f2937"/><rect x="40" y="40" width="1520" height="60" fill="#7aa2f7"/><text x="80" y="480" font-size="64" fill="#f2f3f7" font-family="sans-serif">1600 x 900 screenshot</text></svg>`;
  return btoa(svg);
}

const client = new FakeDaemon().onRequest("get_chat_image", () => ({
  type: "file",
  req_id: "1",
  file: { name: "shot", mime: "image/svg+xml", base64: wideImage(), truncated: false },
}));

const noop = (): void => {};

export const WideScreenshot: Story = () => (
  <Lightbox
    client={client}
    botId="b1"
    images={[
      { id: "a", mime: "image/svg+xml" },
      { id: "b", mime: "image/svg+xml" },
    ]}
    start={0}
    onClose={noop}
  />
);
