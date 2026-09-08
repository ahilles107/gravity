import bloom from "../assets/avatars/bloom.png";
import cloud from "../assets/avatars/cloud.png";
import comet from "../assets/avatars/comet.png";
import copper from "../assets/avatars/copper.png";
import dusk from "../assets/avatars/dusk.png";
import echo from "../assets/avatars/echo.png";
import ember from "../assets/avatars/ember.png";
import frost from "../assets/avatars/frost.png";
import glitch from "../assets/avatars/glitch.png";
import halo from "../assets/avatars/halo.png";
import mint from "../assets/avatars/mint.png";
import moss from "../assets/avatars/moss.png";
import nova from "../assets/avatars/nova.png";
import orbit from "../assets/avatars/orbit.png";
import pixel from "../assets/avatars/pixel.png";
import quartz from "../assets/avatars/quartz.png";
import rune from "../assets/avatars/rune.png";
import slate from "../assets/avatars/slate.png";
import tide from "../assets/avatars/tide.png";
import volt from "../assets/avatars/volt.png";

/**
 * The built-in bot icons, keyed by the name the daemon stores as `icon:<name>`.
 *
 * The daemon validates that name against the same list (`bus::avatar::ICONS`)
 * and hands every new bot one at random, so an avatar is a short string here
 * and the bytes ship with the client. Insertion order is the picker order.
 */
export const BOT_ICONS: Readonly<Record<string, string>> = {
  orbit,
  ember,
  moss,
  nova,
  tide,
  quartz,
  volt,
  dusk,
  copper,
  frost,
  halo,
  glitch,
  slate,
  bloom,
  echo,
  pixel,
  rune,
  cloud,
  comet,
  mint,
};

/** The icon for a stored avatar value, or undefined if it is not an icon. */
export function iconSrc(avatar: string): string | undefined {
  if (!avatar.startsWith("icon:")) {
    return undefined;
  }
  return BOT_ICONS[avatar.slice("icon:".length)];
}
