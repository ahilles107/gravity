import { useCallback } from "react";
import { captureException } from "../../analytics";
import type { DaemonApi } from "../../protocol/api";
import type { NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

type Toast = (level: NotifyLevel, title: string, body: string) => void;

export interface TagAdmin {
  readonly upsert: (name: string, description?: string) => Promise<boolean>;
  readonly rename: (name: string, to: string) => Promise<boolean>;
  readonly remove: (name: string) => Promise<boolean>;
}

/** Owner-side tag maintenance, reloading the registry so counts stay honest. */
export function useTagAdmin(
  client: DaemonApi,
  reload: () => Promise<void>,
  onToast: Toast,
): TagAdmin {
  const run = useCallback(
    async (request: () => Promise<unknown>, failure: string): Promise<boolean> => {
      try {
        await request();
        await reload();
        return true;
      } catch (error) {
        captureException(error, "tag_update");
        onToast("error", failure, errText(error));
        return false;
      }
    },
    [onToast, reload],
  );

  const upsert = useCallback(
    (name: string, description?: string) =>
      run(
        () =>
          client.request(
            {
              type: "upsert_tag",
              name,
              ...(description === undefined ? {} : { description }),
            },
            "tag",
          ),
        "Failed to save the tag",
      ),
    [client, run],
  );

  const rename = useCallback(
    (name: string, to: string) =>
      run(
        () => client.request({ type: "rename_tag", name, to }, "tag"),
        "Failed to rename the tag",
      ),
    [client, run],
  );

  const remove = useCallback(
    (name: string) =>
      run(() => client.request({ type: "delete_tag", name }, "ok"), "Failed to delete the tag"),
    [client, run],
  );

  return { upsert, rename, remove };
}
