import { useEffect, useState } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { FileBody } from "../../protocol/chat";
import type { RequestBody } from "../../protocol/requests";

/** Recently shown images, so scrolling back does not refetch them. */
const CACHE_LIMIT = 64;
const cache = new Map<string, string>();

function remember(key: string, url: string): void {
  cache.delete(key);
  cache.set(key, url);
  const oldest = cache.keys().next();
  if (cache.size > CACHE_LIMIT && oldest.done !== true) {
    cache.delete(oldest.value);
  }
}

export function dataUrl(file: FileBody): string | null {
  return file.base64 === undefined ? null : `data:${file.mime};base64,${file.base64}`;
}

export type DataUrlState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly url: string }
  | { readonly status: "failed" };

/**
 * An image the daemon serves, as a `data:` URL. Images in the chat are only
 * ever bytes the daemon read from the bot's own transcript or files, never a
 * URL from the bot's text.
 */
export function useDataUrl(client: DaemonApi, key: string, request: RequestBody): DataUrlState {
  const cached = cache.get(key);
  const [state, setState] = useState<DataUrlState>(
    cached === undefined ? { status: "loading" } : { status: "ready", url: cached },
  );
  useEffect(() => {
    if (cache.has(key)) {
      return undefined;
    }
    let live = true;
    const load = async (): Promise<void> => {
      try {
        const reply = await client.request(request, "file");
        const url = dataUrl(reply.file);
        if (url !== null) {
          remember(key, url);
        }
        if (live) {
          setState(url === null ? { status: "failed" } : { status: "ready", url });
        }
      } catch {
        if (live) {
          setState({ status: "failed" });
        }
      }
    };
    void load();
    return () => {
      live = false;
    };
    // The request is described by `key`; a new object each render is not a new image.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [client, key]);
  return state;
}
