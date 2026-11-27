import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./boundary";
import { ReadOnlyBanner } from "./readonlybanner";
// the brand's type, bundled (the app's security policy allows no web fonts)
import "@fontsource-variable/bricolage-grotesque/opsz.css";
import "@fontsource-variable/open-sans/index.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";
import "./styles/main.scss";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary
      fallback={() => (
        <div style={{ padding: 32, font: "15px system-ui, sans-serif" }}>
          <b>SketchCraft hit a problem drawing the window.</b>
          <p>Your model is saved. Close and reopen the app (or reload the page).</p>
        </div>
      )}
    >
      <App />
      <ReadOnlyBanner />
    </ErrorBoundary>
  </StrictMode>
);
