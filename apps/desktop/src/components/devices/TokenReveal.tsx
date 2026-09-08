import type { ReactElement } from "react";
import type { NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";
import type { IssuedToken } from "./useDevices";

interface TokenRevealProps {
  readonly issued: IssuedToken;
  readonly onDismiss: () => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/** One-time device token: shown once, never returned by the daemon again. */
export default function TokenReveal({
  issued,
  onDismiss,
  onToast,
}: TokenRevealProps): ReactElement {
  const copy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(issued.token);
      onToast("info", "Token copied", "The device token is on your clipboard.");
    } catch (error) {
      onToast("error", "Copy failed", errText(error));
    }
  };

  return (
    <div className="token-reveal">
      <p className="token-note">
        Token for “{issued.deviceName}” — copy it now, it will not be shown again.
      </p>
      <div className="token-box">
        <input
          readOnly
          aria-label="Device token"
          className="mono"
          value={issued.token}
          onFocus={(event) => {
            event.target.select();
          }}
        />
        <button
          type="button"
          className="btn btn-small"
          onClick={() => {
            void copy();
          }}
        >
          Copy
        </button>
        <button type="button" className="btn btn-small" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}
