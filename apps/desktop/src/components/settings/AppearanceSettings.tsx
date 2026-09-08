import { useState } from "react";
import type { ReactElement } from "react";
import {
  DEFAULT_PREFS,
  MAX_TERMINAL_FONT_SIZE,
  MIN_TERMINAL_FONT_SIZE,
  updatePrefs,
  usePrefs,
} from "../../prefs";

/** Terminal appearance; changes apply live to every open terminal. */
export default function AppearanceSettings(): ReactElement {
  const prefs = usePrefs();
  const [fontFamilyDraft, setFontFamilyDraft] = useState(prefs.terminalFontFamily);
  const isDefault =
    prefs.terminalFontSize === DEFAULT_PREFS.terminalFontSize &&
    prefs.terminalFontFamily === DEFAULT_PREFS.terminalFontFamily;

  const commitFontFamily = (): void => {
    const next = fontFamilyDraft.trim();
    if (next.length === 0) {
      setFontFamilyDraft(prefs.terminalFontFamily);
      return;
    }
    if (next !== fontFamilyDraft) {
      setFontFamilyDraft(next);
    }
    updatePrefs({ terminalFontFamily: next });
  };

  return (
    <div className="settings-section">
      <div className="settings-row">
        <div className="settings-row-text">
          <label className="settings-row-label" htmlFor="settings-font-size">
            Terminal font size
          </label>
          <div className="settings-row-help">
            {MIN_TERMINAL_FONT_SIZE}–{MAX_TERMINAL_FONT_SIZE} px. Applies to open terminals
            immediately.
          </div>
        </div>
        <input
          id="settings-font-size"
          type="number"
          className="settings-number"
          min={MIN_TERMINAL_FONT_SIZE}
          max={MAX_TERMINAL_FONT_SIZE}
          value={prefs.terminalFontSize}
          onChange={(event) => {
            const value = Number.parseInt(event.target.value, 10);
            if (
              Number.isInteger(value) &&
              value >= MIN_TERMINAL_FONT_SIZE &&
              value <= MAX_TERMINAL_FONT_SIZE
            ) {
              updatePrefs({ terminalFontSize: value });
            }
          }}
        />
      </div>

      <div className="settings-row">
        <div className="settings-row-text">
          <label className="settings-row-label" htmlFor="settings-font-family">
            Terminal font
          </label>
          <div className="settings-row-help">
            A CSS font stack; the first installed font wins. Press Enter or leave the field to
            apply.
          </div>
        </div>
        <input
          id="settings-font-family"
          className="settings-wide"
          value={fontFamilyDraft}
          onChange={(event) => {
            setFontFamilyDraft(event.target.value);
          }}
          onBlur={commitFontFamily}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commitFontFamily();
              event.currentTarget.blur();
            }
          }}
        />
      </div>

      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Reset</div>
          <div className="settings-row-help">Back to the bundled appearance defaults.</div>
        </div>
        <button
          type="button"
          className="btn btn-small"
          disabled={isDefault}
          onClick={() => {
            setFontFamilyDraft(DEFAULT_PREFS.terminalFontFamily);
            updatePrefs({
              terminalFontSize: DEFAULT_PREFS.terminalFontSize,
              terminalFontFamily: DEFAULT_PREFS.terminalFontFamily,
            });
          }}
        >
          Reset to defaults
        </button>
      </div>
    </div>
  );
}
