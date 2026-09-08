import { useCallback, useEffect, useState } from "react";
import { capture } from "../analytics";
import type { SettingsCategory } from "../components/settings/categories";

export interface OverlaysApi {
  readonly paletteOpen: boolean;
  readonly searchOpen: boolean;
  readonly searchQuery: string;
  readonly settingsOpen: boolean;
  readonly settingsCategory: SettingsCategory;
  readonly openSearch: (query: string) => void;
  readonly openBlankSearch: () => void;
  readonly closeSearch: () => void;
  readonly closePalette: () => void;
  readonly openSettings: (category?: SettingsCategory) => void;
  readonly closeSettings: () => void;
  readonly selectSettingsCategory: (category: SettingsCategory) => void;
}

/** Command palette, search and settings visibility, plus the ⌘K/⌘, bindings. */
export function useOverlays(): OverlaysApi {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsCategory, setSettingsCategory] = useState<SettingsCategory>("connection");

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (!(event.metaKey || event.ctrlKey)) {
        return;
      }
      if (event.key.toLowerCase() === "k") {
        event.preventDefault();
        event.stopPropagation();
        setSearchOpen(false);
        setSettingsOpen(false);
        setPaletteOpen((prev) => !prev);
      } else if (event.key === ",") {
        event.preventDefault();
        event.stopPropagation();
        setSearchOpen(false);
        setPaletteOpen(false);
        setSettingsOpen((prev) => !prev);
      }
    };
    // Capture shortcuts before xterm's focused textarea can translate them
    // into terminal input for the active bot.
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, []);

  const openSearch = useCallback((query: string): void => {
    capture("search_opened", { source: "command_palette" });
    setPaletteOpen(false);
    setSearchQuery(query);
    setSearchOpen(true);
  }, []);

  const openBlankSearch = useCallback((): void => {
    capture("search_opened", { source: "sidebar" });
    setPaletteOpen(false);
    setSearchQuery("");
    setSearchOpen(true);
  }, []);

  const closeSearch = useCallback((): void => {
    setSearchOpen(false);
  }, []);

  const closePalette = useCallback((): void => {
    setPaletteOpen(false);
  }, []);

  const openSettings = useCallback((category?: SettingsCategory): void => {
    capture("settings_opened", { category: category ?? "connection" });
    setPaletteOpen(false);
    setSearchOpen(false);
    if (category !== undefined) {
      setSettingsCategory(category);
    }
    setSettingsOpen(true);
  }, []);

  const closeSettings = useCallback((): void => {
    setSettingsOpen(false);
  }, []);

  const selectSettingsCategory = useCallback((category: SettingsCategory): void => {
    setSettingsCategory(category);
  }, []);

  return {
    paletteOpen,
    searchOpen,
    searchQuery,
    settingsOpen,
    settingsCategory,
    openSearch,
    openBlankSearch,
    closeSearch,
    closePalette,
    openSettings,
    closeSettings,
    selectSettingsCategory,
  };
}
