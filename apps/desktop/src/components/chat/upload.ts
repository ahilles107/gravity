import type { DaemonApi } from "../../protocol/api";

/** Bytes per chunk: its base64 stays well under the control plane's frame cap. */
const CHUNK_BYTES = 512 * 1024;

function base64(bytes: Uint8Array): string {
  let binary = "";
  const step = 0x8000;
  for (let at = 0; at < bytes.length; at += step) {
    binary += String.fromCharCode(...bytes.subarray(at, at + step));
  }
  return btoa(binary);
}

/**
 * Uploads a file the owner attached into the project's artifacts, one chunk
 * after the other, and returns the path bots can read it at.
 */
export async function uploadAttachment(
  client: DaemonApi,
  projectId: string,
  file: File,
  at = 0,
  uploadId?: string,
): Promise<string> {
  const chunk = new Uint8Array(await file.slice(at, at + CHUNK_BYTES).arrayBuffer());
  const last = at + CHUNK_BYTES >= file.size;
  const reply = await client.request(
    {
      type: "write_artifact",
      project_id: projectId,
      name: file.name,
      base64: base64(chunk),
      last,
      ...(uploadId === undefined ? {} : { upload_id: uploadId }),
    },
    "upload",
  );
  if (!last) {
    return uploadAttachment(client, projectId, file, at + CHUNK_BYTES, reply.upload.upload_id);
  }
  if (reply.upload.path === undefined) {
    throw new Error("the daemon did not say where the file landed");
  }
  return reply.upload.path;
}

/** Types a slash command into the bot's terminal, as if the owner had. */
export function typeIntoTerminal(client: DaemonApi, botId: string, text: string): void {
  // Bracketed paste keeps a multi-line command one input; Enter submits it.
  client.fire({ type: "input", bot_id: botId, data: `\u001b[200~${text}\u001b[201~` });
  window.setTimeout(() => {
    client.fire({ type: "input", bot_id: botId, data: "\r" });
  }, 200);
}
