import type { Story } from "@ladle/react";
import { useRef, useState } from "react";
import type { ReactElement } from "react";
import { decision } from "../../test/decisionFixtures";
import Composer from "./Composer";
import { useComposer } from "./useComposer";

const noop = (): void => {};
const subject = decision({ id: "d1" });

interface DemoProps {
  readonly pick?: string;
  readonly hold?: boolean;
}

/** The composer is driven by `useComposer`, so the stories drive it too. */
function Demo({ pick, hold = false }: DemoProps): ReactElement {
  const composer = useComposer("d1");
  const ref = useRef<HTMLTextAreaElement>(null);
  const [applied, setApplied] = useState(false);
  if (!applied) {
    setApplied(true);
    if (pick !== undefined) {
      composer.togglePick(pick);
    }
    if (hold) {
      composer.setHoldOpen(true);
    }
  }

  return (
    <div className="control-center" style={{ width: 720 }}>
      <Composer
        decision={subject}
        composer={composer}
        canControl
        busy={false}
        textareaRef={ref}
        onSave={noop}
        onAsk={noop}
        onHold={noop}
      />
    </div>
  );
}

export const Blank: Story = () => <Demo />;

export const Picked: Story = () => <Demo pick="start" />;

export const HoldOpen: Story = () => <Demo hold />;
