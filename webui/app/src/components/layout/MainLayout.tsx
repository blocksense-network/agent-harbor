import { Component, JSX } from "solid-js";
import { useLocation, A } from "@solidjs/router";
import agentHarborLogo from "../../assets/agent-harbor-logo.svg";
import { Footer } from "./Footer";
import { useDrafts } from "../../contexts/DraftContext.js";
import { useFocus } from "../../contexts/FocusContext.js";

interface MainLayoutProps {
  children?: JSX.Element;
  onNewDraft?: () => void;
}

export const MainLayout: Component<MainLayoutProps> = (props) => {
  const location = useLocation();
  const { focusState } = useFocus();
  const isActive = (path: string) => location.pathname === path;

  // Access DraftProvider for creating new drafts
  const draftOps = useDrafts();

  const handleNewDraft = async () => {
    console.log("[MainLayout] New Task button clicked");
    // Create a new empty draft
    const created = await draftOps.createDraft({
      prompt: "",
      repo: { mode: "git", url: "", branch: "main" },
      agents: [],
      runtime: { type: "devcontainer" },
      delivery: { mode: "pr" },
    });
    console.log("[MainLayout] Draft creation result:", created);
  };

  return (
    <div class="flex h-screen flex-col bg-white">
      {/* Skip to main content link */}
      <a
        href="#main"
        class={`
          sr-only z-50 rounded-md bg-blue-600 px-4 py-2 text-white
          focus:not-sr-only focus:absolute focus:top-2 focus:left-2
        `}
      >
        Skip to main content
      </a>

      {/* Top Navigation */}
      <header class="border-b border-slate-200 bg-white px-6 py-4 shadow-sm">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-3">
            <img
              src={agentHarborLogo}
              alt="Agent Harbor Logo"
              class="h-8 w-8"
              width="32"
              height="32"
            />
            <div>
              <h1 class="text-xl font-bold text-gray-900">Agent Harbor</h1>
            </div>
          </div>
          <nav class="flex space-x-1" aria-label="Primary">
            <A
              href="/settings"
              class={`
                rounded-lg px-4 py-2 text-sm font-medium transition-colors
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              classList={{
                "bg-slate-100 text-slate-900": isActive("/settings"),
                "text-slate-600 hover:text-slate-900 hover:bg-slate-100":
                  !isActive("/settings"),
              }}
              aria-current={
                location.pathname === "/settings" ? "page" : undefined
              }
            >
              <span aria-hidden="true">⚙️</span> Settings
            </A>
          </nav>
        </div>
      </header>

      {/* Main Content */}
      <main id="main" class="flex-1 overflow-hidden">
        {props.children}
      </main>

      {/* Footer with keyboard shortcuts */}
      <Footer onNewDraft={handleNewDraft} focusState={focusState()} />
    </div>
  );
};
