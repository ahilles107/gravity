/** What the main pane is currently showing. */
export type Selection =
  | { readonly kind: "none" }
  | { readonly kind: "bot"; readonly botId: string }
  | { readonly kind: "project"; readonly projectId: string }
  /**
   * The Control center. `decisionId` opens straight onto one record, which is
   * what a notification or a palette entry hands over.
   */
  | { readonly kind: "control"; readonly decisionId?: string };
