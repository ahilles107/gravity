import type { Story } from "@ladle/react";
import NewProjectForm from "./NewProjectForm";

export const Default: Story = () => (
  <div style={{ width: 280 }}>
    <NewProjectForm onCreate={async (): Promise<void> => {}} onClose={(): void => {}} />
  </div>
);
