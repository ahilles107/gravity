/** What the main pane is currently showing. */
export type Selection =
  | { readonly kind: "none" }
  | { readonly kind: "bot"; readonly botId: string }
  | { readonly kind: "project"; readonly projectId: string };
