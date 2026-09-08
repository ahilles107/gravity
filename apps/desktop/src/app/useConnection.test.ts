import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import { useConnection } from "./useConnection";

describe("useConnection", () => {
  it("clears a stale server version after the connection goes down", () => {
    const daemon = new FakeDaemon();
    const { result } = renderHook(() => useConnection(daemon, vi.fn()));

    expect(result.current.serverVersion).toBe("0.1.0-test");
    act(() => {
      daemon.setStatus("version_mismatch");
    });
    expect(result.current.serverVersion).toBe("");

    daemon.serverVersion = "0.2.0";
    act(() => {
      daemon.setStatus("connected");
    });
    expect(result.current.serverVersion).toBe("0.2.0");
  });
});
