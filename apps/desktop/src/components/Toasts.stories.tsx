import type { Story } from "@ladle/react";
import Toasts from "./Toasts";

const noop = (): void => {};

export const Levels: Story = () => (
  <Toasts
    toasts={[
      {
        id: 1,
        level: "info",
        title: "Token copied",
        body: "The device token is on your clipboard.",
      },
      {
        id: 2,
        level: "warn",
        title: "Read-only",
        body: "This connection lacks the control grant.",
      },
      { id: 3, level: "error", title: "Copy failed", body: "Clipboard permission was denied." },
    ]}
    onDismiss={noop}
  />
);

export const WithAction: Story = () => (
  <Toasts
    toasts={[
      {
        id: 1,
        level: "info",
        title: "Update ready",
        body: "Version 0.7.0 has been downloaded.",
        action: { label: "Restart", run: noop },
      },
    ]}
    onDismiss={noop}
  />
);

export const TitleOnly: Story = () => (
  <Toasts toasts={[{ id: 1, level: "info", title: "Saved", body: "" }]} onDismiss={noop} />
);
