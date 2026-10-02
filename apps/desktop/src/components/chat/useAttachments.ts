import { useRef, useState } from "react";
import { errText } from "../../util";

export interface Attachment {
  readonly key: number;
  readonly name: string;
  /** Null while it uploads. */
  readonly path: string | null;
  readonly error?: string;
}

/** The message text with the attached files' paths appended. */
export function withAttachments(text: string, attachments: readonly Attachment[]): string {
  const paths = attachments.flatMap((a) => (a.path === null ? [] : [a.path]));
  if (paths.length === 0) {
    return text;
  }
  const listing = paths.map((path) => `- ${path}`).join("\n");
  return text === "" ? `Attached files:\n${listing}` : `${text}\n\nAttached files:\n${listing}`;
}

export interface Attachments {
  readonly list: readonly Attachment[];
  /** Uploaded and ready to send. */
  readonly ready: readonly Attachment[];
  readonly uploading: boolean;
  readonly attach: (files: readonly File[]) => void;
  readonly remove: (key: number) => void;
  readonly clear: () => void;
}

/** Files attached to the draft, each uploading as soon as it is added. */
export function useAttachments(upload: ((file: File) => Promise<string>) | undefined): Attachments {
  const [list, setList] = useState<readonly Attachment[]>([]);
  const nextKey = useRef(0);
  const update = (key: number, change: Partial<Attachment>): void => {
    setList((current) => current.map((a) => (a.key === key ? { ...a, ...change } : a)));
  };
  const attach = (files: readonly File[]): void => {
    if (upload === undefined) {
      return;
    }
    for (const file of files) {
      const key = nextKey.current++;
      setList((current) => [...current, { key, name: file.name, path: null }]);
      void upload(file)
        .then((path) => {
          update(key, { path });
          return path;
        })
        .catch((failure: unknown) => {
          update(key, { error: errText(failure) });
        });
    }
  };
  return {
    list,
    ready: list.filter((a) => a.path !== null),
    uploading: list.some((a) => a.path === null && a.error === undefined),
    attach,
    remove: (key) => {
      setList((current) => current.filter((a) => a.key !== key));
    },
    clear: () => {
      setList([]);
    },
  };
}
