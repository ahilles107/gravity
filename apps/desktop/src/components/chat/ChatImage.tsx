import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { ImageRef } from "../../protocol/chat";
import Lightbox from "./Lightbox";
import { useDataUrl } from "./useDataUrl";

interface ChatImageProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly image: ImageRef;
  readonly onOpen: () => void;
}

/** A thumbnail of an image a step produced, at its own shape. */
function ChatImage({ client, botId, image, onOpen }: ChatImageProps): ReactElement {
  const state = useDataUrl(client, `${botId}:${image.id}`, {
    type: "get_chat_image",
    bot_id: botId,
    image_id: image.id,
  });
  if (state.status !== "ready") {
    return (
      <span className="chat-thumb chat-thumb-empty">
        {state.status === "loading" ? "Loading image…" : "Image unavailable"}
      </span>
    );
  }
  return (
    <button type="button" className="chat-thumb" aria-label="Open image" onClick={onOpen}>
      <img src={state.url} alt="" />
    </button>
  );
}

interface ImageStripProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly images: readonly ImageRef[];
}

/** A step group's images; any of them opens the lightbox, which pages through all. */
export function ImageStrip({ client, botId, images }: ImageStripProps): ReactElement | null {
  const [open, setOpen] = useState<number | null>(null);
  if (images.length === 0) {
    return null;
  }
  return (
    <div className="chat-image-strip">
      {images.map((image, index) => (
        <ChatImage
          key={image.id}
          client={client}
          botId={botId}
          image={image}
          onOpen={() => {
            setOpen(index);
          }}
        />
      ))}
      {open === null ? null : (
        <Lightbox
          client={client}
          botId={botId}
          images={images}
          start={open}
          onClose={() => {
            setOpen(null);
          }}
        />
      )}
    </div>
  );
}
