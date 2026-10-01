import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { ImageRef } from "../../protocol/chat";
import { useDataUrl } from "./useDataUrl";

interface LightboxProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly images: readonly ImageRef[];
  readonly start: number;
  readonly onClose: () => void;
}

/**
 * Screenshots full size: fitted to the window at their own shape, or at
 * actual size to read the detail. ←/→ move between a step group's images,
 * Esc or the backdrop closes.
 */
export default function Lightbox(props: LightboxProps): ReactElement {
  const { client, botId, images, onClose } = props;
  const [index, setIndex] = useState(props.start);
  const [actual, setActual] = useState(false);
  const count = images.length;
  const image = images[index];
  const state = useDataUrl(client, `${botId}:${image?.id ?? ""}`, {
    type: "get_chat_image",
    bot_id: botId,
    image_id: image?.id ?? "",
  });

  const step = (by: number): void => {
    setActual(false);
    setIndex((current) => (current + by + count) % count);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onClose();
      } else if (event.key === "ArrowRight" && count > 1) {
        setActual(false);
        setIndex((current) => (current + 1) % count);
      } else if (event.key === "ArrowLeft" && count > 1) {
        setActual(false);
        setIndex((current) => (current - 1 + count) % count);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose, count]);

  return (
    // The backdrop, the bar and the stage's empty space close it, like any overlay.
    <div
      className="lightbox"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target instanceof HTMLElement && event.target.dataset.backdrop === "true") {
          onClose();
        }
      }}
    >
      <div
        className="lightbox-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Image"
        data-backdrop="true"
      >
        <div className="lightbox-bar" data-backdrop="true">
          <span className="lightbox-count">{count > 1 ? `${index + 1} of ${count}` : ""}</span>
          <span className="lightbox-hint">
            {actual ? "Actual size" : "Fitted"} · click the image to switch
          </span>
          <button type="button" className="icon-btn" aria-label="Close" onClick={onClose}>
            <X size={18} aria-hidden="true" />
          </button>
        </div>
        <div className={`lightbox-stage${actual ? " lightbox-actual" : ""}`} data-backdrop="true">
          {state.status === "ready" ? (
            <button
              type="button"
              className="lightbox-image"
              aria-label={actual ? "Fit to the window" : "Show at actual size"}
              onClick={() => {
                setActual((current) => !current);
              }}
            >
              <img src={state.url} alt="" />
            </button>
          ) : (
            <span className="lightbox-note">
              {state.status === "loading" ? "Loading image…" : "Image unavailable"}
            </span>
          )}
        </div>
        {count > 1 ? (
          <>
            <button
              type="button"
              className="icon-btn lightbox-nav lightbox-prev"
              aria-label="Previous image"
              onClick={() => {
                step(-1);
              }}
            >
              <ChevronLeft size={28} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="icon-btn lightbox-nav lightbox-next"
              aria-label="Next image"
              onClick={() => {
                step(1);
              }}
            >
              <ChevronRight size={28} aria-hidden="true" />
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
