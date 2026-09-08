import { useState } from "react";
import type { Story } from "@ladle/react";
import type { BotTab } from "./BotTabs";
import BotTabs from "./BotTabs";

export const Interactive: Story = () => {
  const [active, setActive] = useState<BotTab>("terminal");
  return <BotTabs active={active} onSelect={setActive} />;
};
