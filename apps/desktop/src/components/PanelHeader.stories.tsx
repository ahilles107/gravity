import type { Story } from "@ladle/react";
import PanelHeader from "./PanelHeader";

export const TitleOnly: Story = () => <PanelHeader title="Devices" />;

export const WithAction: Story = () => (
  <PanelHeader title="Routines">
    <button type="button" className="btn btn-small btn-primary">
      New routine
    </button>
  </PanelHeader>
);
