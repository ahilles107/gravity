import { afterEach, describe, expect, it, vi } from "vitest";

const render = vi.hoisted(() => vi.fn<(node: unknown) => void>());
const createRoot = vi.hoisted(() =>
  vi.fn<(container: Element) => { render: (node: unknown) => void }>(() => ({ render })),
);

vi.mock("react-dom/client", () => ({ createRoot }));
vi.mock("./App", () => ({ default: (): null => null }));

afterEach(() => {
  vi.resetModules();
  createRoot.mockClear();
  render.mockClear();
  document.body.innerHTML = "";
});

describe("main", () => {
  it("mounts the app into #root", async () => {
    const root = document.createElement("div");
    root.id = "root";
    document.body.append(root);

    await import("./main");

    expect(createRoot).toHaveBeenCalledWith(root);
    expect(render).toHaveBeenCalled();
  });

  it("throws when the mount point is missing", async () => {
    await expect(import("./main")).rejects.toThrow("missing #root element");
  });
});
