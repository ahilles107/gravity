import type { ReactElement } from "react";
import { iconSrc } from "./botIcons";

/**
 * A bot's avatar: one of the built-in icons, a flat colour swatch, or — when
 * unset — a swatch derived from the bot id so every bot still reads as
 * distinct.
 *
 * The stored value is a short validated string (`icon:` or `color:`), never a
 * URL or data URI, so nothing here fetches or renders untrusted bytes: an
 * `icon:` name only ever resolves to an asset bundled with the client.
 */

interface BotAvatarProps {
  readonly avatar: string;
  /** Used for the initial and, when no avatar is set, the fallback colour. */
  readonly name: string;
  readonly id: string;
  readonly size?: "sm" | "md" | "lg";
  /** Dim the avatar for an archived bot. */
  readonly muted?: boolean;
}

/** Deterministic hue from an id, so a bot's fallback colour never changes. */
function hueFromId(id: string): number {
  let hash = 0;
  for (const char of id) {
    hash = (hash * 31 + (char.codePointAt(0) ?? 0)) % 360;
  }
  return hash;
}

function initial(name: string): string {
  return [...name.trim()][0]?.toUpperCase() ?? "?";
}

export default function BotAvatar({
  avatar,
  name,
  id,
  size = "md",
  muted = false,
}: BotAvatarProps): ReactElement {
  const className = `bot-avatar bot-avatar-${size}${muted ? " bot-avatar-muted" : ""}`;

  // An unknown icon name falls through to the swatch rather than rendering a
  // hole, which is what a client older than the daemon would otherwise show.
  const src = iconSrc(avatar);
  if (src !== undefined) {
    return <img className={className} src={src} alt="" aria-hidden="true" />;
  }

  const background = avatar.startsWith("color:")
    ? avatar.slice("color:".length)
    : `hsl(${hueFromId(id)} 45% 42%)`;

  return (
    <span className={className} style={{ background }} aria-hidden="true">
      {initial(name)}
    </span>
  );
}
