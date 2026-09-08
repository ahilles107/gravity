import type { Story } from "@ladle/react";
import BotAvatar from "./BotAvatar";

export const Sizes: Story = () => (
  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
    <BotAvatar avatar="" name="alice" id="b1" size="sm" />
    <BotAvatar avatar="" name="alice" id="b1" size="md" />
    <BotAvatar avatar="" name="alice" id="b1" size="lg" />
  </div>
);

export const Icon: Story = () => (
  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
    <BotAvatar avatar="icon:orbit" name="orbit" id="b1" size="lg" />
    <BotAvatar avatar="icon:ember" name="ember" id="b2" size="lg" />
    <BotAvatar avatar="icon:frost" name="frost" id="b3" size="lg" />
  </div>
);

export const ColorSwatch: Story = () => (
  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
    <BotAvatar avatar="color:#7aa2f7" name="blue" id="b1" size="lg" />
    <BotAvatar avatar="color:#f87171" name="red" id="b2" size="lg" />
    <BotAvatar avatar="color:#4ade80" name="green" id="b3" size="lg" />
  </div>
);

export const FallbackFromId: Story = () => (
  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
    {["b1", "b2", "b3", "b4", "b5"].map((id) => (
      <BotAvatar key={id} avatar="" name={id} id={id} size="lg" />
    ))}
  </div>
);

export const Muted: Story = () => (
  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
    <BotAvatar avatar="icon:orbit" name="orbit" id="b1" size="lg" muted />
    <BotAvatar avatar="" name="alice" id="b1" size="lg" muted />
  </div>
);
