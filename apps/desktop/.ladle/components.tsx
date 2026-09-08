import type { GlobalProvider } from "@ladle/react";
import "../src/styles.css";
import "./theme.css";

export const Provider: GlobalProvider = ({ children }) => <>{children}</>;
