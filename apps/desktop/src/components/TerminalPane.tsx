import { useEffect, useRef } from "react";
import type { ReactElement } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal } from "@xterm/xterm";
import type { ITerminalOptions } from "@xterm/xterm";
import { captureException } from "../analytics";
import type { AddToast } from "../app/useToasts";
import { openExternalUrl } from "../externalUrl";
import { getPrefs, subscribePrefs } from "../prefs";
import type { DaemonApi } from "../protocol/api";
import { errText } from "../util";
import { storeCachedTerminal, takeCachedTerminal } from "./terminalCache";
import type { CachedTerminal } from "./terminalCache";
import { listenForTerminalFileDrops } from "./terminalFileDrop";
import { loadWebgl } from "./terminalRenderer";

interface TerminalPaneProps {
  readonly client: DaemonApi;
  readonly botId: string;
  /** Whether typing/resizing is forwarded: bot running + `control` grant. */
  readonly canWrite: boolean;
  readonly onToast?: AddToast;
}

type LinkHandler = NonNullable<ITerminalOptions["linkHandler"]>;

/**
 * A cached terminal outlives the pane that built it, so its link handler
 * reports through a slot the mounted pane keeps pointed at the live toaster.
 */
const linkFailure: { report: (body: string) => void } = { report: () => {} };

function activateLink(_event: MouseEvent, uri: string): void {
  openExternalUrl(uri).catch((error: unknown) => {
    linkFailure.report(error instanceof Error ? error.message : `${uri} could not be opened.`);
  });
}

/**
 * An OSC 8 hyperlink carries a target its displayed text need not match, so
 * output can print one host and navigate to another. xterm guards that with a
 * confirm dialog only while no `linkHandler` is set; supplying one means
 * showing where the link actually goes before the user commits to it.
 */
function createLinkHandler(host: () => HTMLElement): LinkHandler {
  let tooltip: HTMLElement | null = null;
  const hide = (): void => {
    tooltip?.remove();
    tooltip = null;
  };
  return {
    activate: activateLink,
    hover: (event: MouseEvent, uri: string): void => {
      hide();
      const parent = host();
      const node = document.createElement("div");
      node.className = "terminal-link-target";
      node.textContent = uri;
      const bounds = parent.getBoundingClientRect();
      node.style.left = `${event.clientX - bounds.left}px`;
      node.style.top = `${event.clientY - bounds.top}px`;
      parent.appendChild(node);
      tooltip = node;
    },
    leave: hide,
  };
}

/**
 * The element is created imperatively rather than rendered by React so it can
 * be re-parented into a later pane's container with the xterm instance — and
 * its screen — intact.
 */
function createTerminal(): CachedTerminal {
  const element = document.createElement("div");
  element.style.width = "100%";
  element.style.height = "100%";
  const term: Terminal = new Terminal({
    fontFamily: getPrefs().terminalFontFamily,
    fontSize: getPrefs().terminalFontSize,
    lineHeight: 1.2,
    scrollback: 8000,
    cursorBlink: true,
    // `term.element` only exists once opened, so the host is resolved lazily.
    linkHandler: createLinkHandler(() => term.element ?? element),
    theme: {
      background: "#101116",
      foreground: "#d8dae2",
      cursor: "#7aa2f7",
      selectionBackground: "#33405e",
    },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.loadAddon(new WebLinksAddon(activateLink));
  return { term, fit, element };
}

/** Modal dialogs own keyboard input while open, even if xterm retained focus. */
function terminalInputAllowed(canWrite: boolean): boolean {
  return canWrite && document.querySelector('[aria-modal="true"]') === null;
}

/**
 * xterm.js pane attached to a bot terminal via `attach` / `term` frames.
 * The terminal belongs entirely to the user: any connection with the `control`
 * grant may send `input`/`resize` while the bot runs (bus deliveries go to the
 * bot's inbox socket and never touch the pty). Read-only connections stay
 * view-only; the server would reject their input with `forbidden`.
 */
export default function TerminalPane({
  client,
  botId,
  canWrite,
  onToast,
}: TerminalPaneProps): ReactElement {
  const connectionGeneration = client.connectionGeneration;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canWriteRef = useRef(canWrite);
  const termRef = useRef<Terminal | null>(null);
  // Seeded with the mount-time value so only a real false→true transition
  // (bot restarted, grant arrived) forces a pty repaint below; the attach
  // flow already handles the mount itself.
  const wasWritable = useRef(canWrite);

  useEffect(() => {
    canWriteRef.current = canWrite;
  }, [canWrite]);

  useEffect(() => {
    linkFailure.report = (body: string): void => {
      onToast?.("warn", "Could not open link", body);
    };
    return () => {
      linkFailure.report = (): void => {};
    };
  }, [onToast]);

  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return;
    }
    return listenForTerminalFileDrops({
      element: container,
      canDrop: () => terminalInputAllowed(canWriteRef.current),
      onDrop: (input) => {
        client.fire({ type: "input", bot_id: botId, data: input });
        termRef.current?.focus();
      },
      onError: (error) => {
        captureException(error, "terminal_file_drop");
        onToast?.("error", "File drop unavailable", errText(error));
      },
    });
  }, [botId, client, onToast]);

  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return;
    }

    const cached = takeCachedTerminal(client, connectionGeneration, botId);
    const entry = cached ?? createTerminal();
    const { term, fit, element } = entry;
    container.appendChild(element);
    if (cached === undefined) {
      term.open(element);
      loadWebgl(term, element);
    }
    termRef.current = term;

    const safeFit = (): void => {
      if (container.clientWidth === 0 || container.clientHeight === 0) {
        return;
      }
      fit.fit();
    };
    safeFit();
    if (cached !== undefined) {
      // The renderer's canvas does not survive re-parenting untouched.
      term.refresh(0, term.rows - 1);
    }
    if (canWriteRef.current) {
      term.focus();
    }

    const sendResize = (force: boolean): void => {
      if (canWriteRef.current) {
        client.fire({ type: "resize", bot_id: botId, cols: term.cols, rows: term.rows, force });
      }
    };

    // xterm reports a bare CR for Enter whatever modifiers are held, so
    // Shift+Enter submits the prompt instead of inserting a newline. Claude
    // Code reads ESC+CR — what a meta-aware terminal emits for Option+Enter —
    // as "newline, don't submit", so translate both chords into that.
    term.attachCustomKeyEventHandler((event) => {
      const wantsNewline =
        event.type === "keydown" &&
        event.key === "Enter" &&
        (event.shiftKey || event.altKey) &&
        !event.ctrlKey &&
        !event.metaKey;
      if (!wantsNewline) {
        return true;
      }
      // xterm skips the key once we return false, but the browser would still
      // let it reach the hidden textarea xterm reads input from.
      event.preventDefault();
      if (terminalInputAllowed(canWriteRef.current)) {
        client.fire({ type: "input", bot_id: botId, data: "\u001b\r" });
      }
      return false;
    });

    const dataSub = term.onData((data) => {
      if (terminalInputAllowed(canWriteRef.current)) {
        client.fire({ type: "input", bot_id: botId, data });
      }
    });

    // Sequence number of the last frame of an in-flight full replay; 0 when
    // no replay is pending. A bulk replay overruns the scrollback cap, and
    // xterm's trimming can leave the viewport parked mid-buffer instead of
    // following the output, so the final replay write re-pins it once the
    // batch has been processed. Live frames keep xterm's own follow behaviour.
    let replayThrough = 0;
    const unsubTerm = client.on("term", (push) => {
      if (push.bot_id !== botId) {
        return;
      }
      if (replayThrough > 0) {
        const finishesReplay = push.seq >= replayThrough;
        if (finishesReplay) {
          replayThrough = 0;
        }
        if (finishesReplay) {
          term.write(push.data, () => {
            term.scrollToBottom();
          });
        } else {
          term.write(push.data);
        }
      } else {
        term.write(push.data);
      }
    });

    // A cached terminal still shows the screen it had when the user left, so
    // its attach may resume: the server then replays only what was missed. A
    // terminal created empty must take the whole ring instead — the client's
    // cursor outlives individual panes, and resuming from it would leave
    // everything before it blank.
    let resumable = cached !== undefined;
    let disposed = false;
    const doAttach = async (): Promise<void> => {
      try {
        const { resumed, seq } = await client.attach(botId, resumable);
        // Switching bots (and every StrictMode remount) tears this pane down
        // while its attach is still in flight: the terminal below is parked,
        // and the resize would only make the runtime repaint for a pane nobody
        // is looking at.
        if (disposed) {
          return;
        }
        resumable = true;
        if (!resumed) {
          // Replaying from the top of the buffer paints over whatever is here.
          term.reset();
          // Everything up to the attach-time sequence number is replay; the
          // push handler above follows it to the bottom of the screen.
          replayThrough = seq;
          // Force a repaint: the replay rebuilds the screen out of what the
          // ring still holds, and an unchanged size would otherwise leave the
          // runtime silent about the live region it owns. A resumed replay is
          // contiguous with the screen this terminal kept, so it needs none of
          // this.
          sendResize(true);
        }
      } catch {
        // Not connected yet; the status listener below retries on reconnect.
      }
    };
    void doAttach();

    const unsubStatus = client.onStatus((status) => {
      if (status === "connected") {
        void doAttach();
      }
    });

    const observer = new ResizeObserver(() => {
      safeFit();
      sendResize(false);
    });
    observer.observe(container);

    // A cached terminal keeps the font it was created with; sync it (and any
    // later change from the settings overlay) to the current preferences.
    const applyFont = (): void => {
      const prefs = getPrefs();
      if (
        term.options.fontSize !== prefs.terminalFontSize ||
        term.options.fontFamily !== prefs.terminalFontFamily
      ) {
        term.options.fontSize = prefs.terminalFontSize;
        term.options.fontFamily = prefs.terminalFontFamily;
        safeFit();
        sendResize(false);
      }
    };
    applyFont();
    const unsubPrefs = subscribePrefs(applyFont);

    return () => {
      disposed = true;
      unsubPrefs();
      observer.disconnect();
      unsubStatus();
      unsubTerm();
      dataSub.dispose();
      void client.request({ type: "detach", bot_id: botId }, "ok").catch(() => undefined);
      termRef.current = null;
      element.remove();
      // The instance outlives the pane: switching back to a still-warm bot
      // resumes instantly instead of replaying the whole ring.
      storeCachedTerminal(client, connectionGeneration, botId, entry);
    };
  }, [botId, client, connectionGeneration]);

  // When the pane becomes writable, sync the server pty to our current size.
  // The bot's runtime is new (restart) or newly ours (grant), so the sync is
  // forced: its pty may match our size yet show a screen we never received.
  useEffect(() => {
    const term = termRef.current;
    const becameWritable = canWrite && !wasWritable.current;
    wasWritable.current = canWrite;
    if (becameWritable && term !== null) {
      client.fire({
        type: "resize",
        bot_id: botId,
        cols: term.cols,
        rows: term.rows,
        force: true,
      });
      term.focus();
    }
  }, [botId, client, canWrite]);

  return (
    <div className={`terminal-wrap ${canWrite ? "" : "terminal-readonly"}`}>
      <div ref={containerRef} className="terminal-host" />
    </div>
  );
}
