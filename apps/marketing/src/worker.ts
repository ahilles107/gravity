import { resolveMacDownloadUrl } from "./release";

interface Env {
  readonly ASSETS: Fetcher;
  readonly RELEASE_MANIFEST_URL?: string;
}

async function latestDownload(manifestUrl: string | undefined): Promise<Response> {
  if (!manifestUrl?.trim()) {
    return new Response("Downloads are not configured for this deployment.", { status: 503 });
  }
  const manifestResponse = await fetch(manifestUrl, {
    cf: {
      cacheEverything: true,
      cacheTtl: 300,
    },
  });

  if (!manifestResponse.ok) {
    return new Response("The latest Gravity download is temporarily unavailable.", {
      status: 502,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }

  const downloadUrl = resolveMacDownloadUrl(await manifestResponse.json());
  if (downloadUrl === null) {
    return new Response("The latest Gravity release does not include a macOS download.", {
      status: 502,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }

  return Response.redirect(downloadUrl, 302);
}

const worker: ExportedHandler<Env> = {
  async fetch(request, env): Promise<Response> {
    const url = new URL(request.url);
    if (request.method === "GET" && url.pathname === "/download/latest") {
      return latestDownload(env.RELEASE_MANIFEST_URL);
    }

    return env.ASSETS.fetch(request);
  },
};

export default worker;
