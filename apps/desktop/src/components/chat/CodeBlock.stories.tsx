import type { Story } from "@ladle/react";
import CodeBlock from "./CodeBlock";
import DiffView from "./DiffView";

export const TypeScript: Story = () => (
  <div style={{ width: 640 }}>
    <CodeBlock
      language="ts"
      code={
        "// Merge pushed turns by id.\nexport function merge(a: Turn[], b: Turn[]): Turn[] {\n  const limit = 30;\n  return [...a, ...b].slice(-limit); // newest last\n}"
      }
    />
  </div>
);

export const Diff: Story = () => (
  <div style={{ width: 640 }}>
    <DiffView
      lines={[
        "@@ -10,3 +10,4 @@",
        " fn install() {",
        "-    copy(src, dst)?;",
        "+    let staged = stage(src)?;",
        "+    replace(staged, dst)?;",
        " }",
      ]}
    />
  </div>
);
