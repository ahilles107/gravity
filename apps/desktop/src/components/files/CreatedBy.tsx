import type { ReactElement } from "react";
import type { FileCreator } from "../../protocol/chat";
import BotAvatar from "../BotAvatar";

const VIA: Readonly<Record<FileCreator["via"], string>> = {
  wrote: "written by",
  edited: "first edited by",
  command: "made by a command of",
  upload: "uploaded by",
  sent: "sent by",
};

/** "written by lead", "sent by windev @ win-pc", with the bot's face. */
export function createdByLabel(creator: FileCreator): string {
  const who = creator.machine == null ? creator.name : `${creator.name} @ ${creator.machine}`;
  return `${VIA[creator.via]} ${who}`;
}

/** Who made a file, under its name in the Files tab. */
export default function CreatedBy({ creator }: { readonly creator: FileCreator }): ReactElement {
  return (
    <span className="files-creator">
      {creator.bot_id == null ? null : (
        <BotAvatar avatar={creator.avatar} name={creator.name} id={creator.bot_id} size="sm" />
      )}
      <span>{createdByLabel(creator)}</span>
    </span>
  );
}
