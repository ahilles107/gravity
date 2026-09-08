import type { Story } from "@ladle/react";
import { bot } from "../../test/fixtures";
import CreateRoutineForm from "./CreateRoutineForm";

export const Default: Story = () => (
  <div style={{ maxWidth: 480 }}>
    <CreateRoutineForm
      bots={[bot({ id: "b1", name: "alice" }), bot({ id: "b2", name: "bob" })]}
      onSubmit={async (): Promise<boolean> => true}
      onClose={(): void => {}}
    />
  </div>
);
