import { Router } from "@solidjs/router";
import { FileRoutes } from "@solidjs/start/router";
import { MetaProvider, Title, Meta } from "@solidjs/meta";
import { MainLayout } from "./components/layout/MainLayout.js";
import { SessionProvider } from "./contexts/SessionContext.js";
import { DraftProvider } from "./contexts/DraftContext.js";
import { FocusProvider } from "./contexts/FocusContext.js";
import { ToastProvider } from "./contexts/ToastContext.js";
import { BreadcrumbProvider } from "./contexts/BreadcrumbContext.js";
import { isServer, getRequestEvent } from "solid-js/web";
import "./app.css";

export default function App() {
  let initialUrl = "";
  if (isServer) {
    const event = getRequestEvent();
    if (event) {
      initialUrl = event.request.url;
    }
  }

  return (
    <MetaProvider>
      <Title>Agent Harbor</Title>
      <Meta
        name="description"
        content="Create and manage AI agent coding sessions"
      />
      <Meta name="viewport" content="width=device-width, initial-scale=1.0" />
      <ToastProvider>
        <SessionProvider>
          <DraftProvider>
            <FocusProvider>
              <BreadcrumbProvider>
                <Router
                  url={initialUrl}
                  root={(props) => <MainLayout>{props.children}</MainLayout>}
                >
                  <FileRoutes />
                </Router>
              </BreadcrumbProvider>
            </FocusProvider>
          </DraftProvider>
        </SessionProvider>
      </ToastProvider>
    </MetaProvider>
  );
}
