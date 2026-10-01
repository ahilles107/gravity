import { useEffect, useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { ImageRef } from "../../protocol/chat";
import OverlayShell from "../overlay/OverlayShell";
import { useDataUrl } from "./useDataUrl";

interface ChatImageProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly image: ImageRef;
}

/** A thumbnail of an image a step produced; click to view it full size. */
function ChatImage({ client, botId, image }: ChatImageProps): ReactElement {
  const [open, setOpen] = useState(false);
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
    <>
      <button
        type="button"
        className="chat-thumb"
        aria-label="Open image"
        onClick={() => {
          setOpen(true);
        }}
      >
        <img src={state.url} alt="" />
      </button>
      {open ? (
        <ImageViewer
          url={state.url}
          onClose={() => {
            setOpen(false);
          }}
        />
      ) : null}
    </>
  );
}

interface ImageViewerProps {
  readonly url: string;
  readonly onClose: () => void;
}

function ImageViewer({ url, onClose }: ImageViewerProps): ReactElement {
  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);
  return (
    <OverlayShell label="Image" onClose={onClose}>
      <div className="image-viewer">
        <img src={url} alt="" />
        <button type="button" className="btn btn-small" onClick={onClose}>
          Close
        </button>
      </div>
    </OverlayShell>
  );
}

interface ImageStripProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly images: readonly ImageRef[];
}

export function ImageStrip({ client, botId, images }: ImageStripProps): ReactElement | null {
  if (images.length === 0) {
    return null;
  }
  return (
    <div className="chat-image-strip">
      {images.map((image) => (
        <ChatImage key={image.id} client={client} botId={botId} image={image} />
      ))}
    </div>
  );
}
