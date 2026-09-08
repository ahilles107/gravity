import { proxyUpdateManifest } from "./updater-proxy";

interface Env {
  readonly RELEASE_MANIFEST_URL?: string;
}

export default {
  fetch(request: Request, env: Env): Promise<Response> {
    return proxyUpdateManifest(request, env.RELEASE_MANIFEST_URL);
  },
} satisfies ExportedHandler<Env>;
