import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { HelpWindow } from "./HelpWindow";
import "./styles.css";

function checkIsHelp(): boolean {
  if (typeof window !== "undefined") {
    if ((window as any).__IS_HELP_WINDOW__) return true;
    try {
      const internals = (window as any).__TAURI_INTERNALS__;
      if (internals?.metadata?.currentWindow?.label === "help") {
        return true;
      }
    } catch {
      // ignore
    }
    if (
      window.location.search.includes("help") ||
      window.location.hash.includes("help") ||
      window.location.pathname.includes("help")
    ) {
      return true;
    }
  }
  return false;
}

const isHelpView = checkIsHelp();

const App = lazy(() => import("./App"));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isHelpView ? (
      <HelpWindow />
    ) : (
      <Suspense fallback={<div className="loading-fallback" />}>
        <App />
      </Suspense>
    )}
  </React.StrictMode>
);
