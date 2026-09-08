import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { AnalyticsProvider } from "./analytics";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("missing #root element");
}

createRoot(container).render(
  <StrictMode>
    <AnalyticsProvider>
      <App />
    </AnalyticsProvider>
  </StrictMode>,
);
