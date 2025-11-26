## Docs Site — Product Requirements and UX Specification

### Summary

The Docs Site is the primary documentation hub for Agent Harbor and its ecosystem (CLI/TUI, WebUI, AgentFS, Recorder, API, development workflows).

It is implemented as a **static documentation site** using **Next.js 15 + Nextra 4 (Docs theme)** with filesystem-based routing under `apps/docs.agent-harbor.dev/app`. Content is authored in Markdown/MDX inside the main repository and presented as a navigable, searchable experience.

- `apps/docs.agent-harbor.dev/app/layout.tsx` composes the Nextra `<Layout>` and `<Navbar>` with `getPageMap` to auto-wire sidebar/page navigation.
- `apps/docs.agent-harbor.dev/mdx-components.tsx` merges Nextra theme components with custom MDX components, so bespoke blocks can be added without losing defaults.
- Search uses the **built-in Nextra Docs local search**, bundled into the static export without external services.
- `apps/docs.agent-harbor.dev/app/page.mdx` is the homepage entrypoint; additional sections should be represented as sibling MDX files/directories (App Router semantics).

Unless noted otherwise, layout, navigation, theming, and search use **Nextra Docs default behavior** with configuration limited to branding, ordering, and content.

The site serves:

- People discovering Agent Harbor for the first time.
- Engineers and operators using TUI, WebUI, and the REST service.
- Contributors working on the codebase and internal specs.

### Goals

- Provide a **single, discoverable entry point** for all Agent Harbor documentation.
- Keep **in-repo markdown as the source of truth**, rendered through the docs site.
- Make it easy to **get started quickly** (Quick Start) and then dive deeper (TUI, WebUI, AgentFS, Recorder, API).
- Offer **fast navigation and search** across all docs.
- Ensure **documentation stays aligned with releases** by tying docs to shipped features.
- Give contributors a **clear, low-friction workflow** for adding and updating docs.

### Non-Goals

- Moving documentation to a separate repository.
- Building a forum, Q&A system, or support desk into the docs site.
- Implementing complex release/version-management workflows in v1 (simple version labeling and links are enough).
- Replacing generated API reference systems (if any) — the docs site can link to them.

### Audience and Key Use Cases

#### Audience

- Engineers using the CLI/TUI/WebUI.
- Operators running Agent Harbor in local or remote/server mode.
- Contributors and maintainers.
- Evaluators landing from GitHub or external links.

#### Key Use Cases

1. **Landing from README**
   - User clicks a “Documentation” link in the repository README.
   - Lands on docs homepage with a clear explanation of Agent Harbor and a visible Quick Start path.

2. **Quick Start**
   - User finds installation and “first successful run” instructions.
   - Can complete an end-to-end minimal workflow on their machine (e.g., local mode).

3. **Feature Deep Dives**
   - User navigates to focused sections:
     - TUI behavior.
     - WebUI concepts and flows.
     - AgentFS and Recorder integration.
     - REST API concepts and example flows.

4. **Operations & Integration**
   - Operator finds deployment and operations notes.
   - Engineer finds how to integrate with the API and existing tools.

5. **Contributing**
   - Potential contributor finds:
     - Repo layout overview.
     - How to run the docs locally.
     - How to add/edit pages.
     - Style guidelines and expectations.

6. **Search & Discovery**
   - User types “AgentFS” or “Recorder” or “local mode” into search.
   - Sees relevant pages, with enough context to pick the right one.

### Information Architecture

The docs site organizes content into a small set of **top-level sections** in the sidebar, including (but not limited to):

- **Intro / Overview**
- **Quick Start**
- **TUI**
- **WebUI**
- **AgentFS**
- **Recorder**
- **API**
- **Development Guides / Contributing**

The exact nesting and page grouping can evolve, but:

- Each major capability has a clearly discoverable home page.
- Overview and Quick Start are always visible near the top of the navigation.
- Internal specs/PRDs can be referenced from a “Development / Internal docs” area, without overwhelming end users.

### Page Inventory and Layout Details

| Page                       | Layout and content expectations                                                                                                                                                                                                                                                          | Source / notes                                                                                   |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Homepage (`app/page.mdx`)  | Hero with 1–2 line value statement, “Get started” CTA, trio of tiles linking to Quick Start, TUI, WebUI. Include a short “What’s in this site” list and a surface to recent/featured updates.                                                                                            | Pull messaging from README and Product One Pager; keep tiles in sync with Quick Start / UI docs. |
| Quick Start                | Task-focused walkthrough that gets a user to first successful run (local mode) in ≤5 minutes. Include prerequisites, `just` commands or `yarn dev` where relevant, screenshots or asciinema links when available, and a “Verify success” checklist. Cross-link to TUI/WebUI to continue. | Based on README and current CLI/TUI/WebUI flows; keep commands tested.                           |
| TUI                        | Describe navigation and common workflows. Provide “keyboard first” tips, failure recovery steps, and highlight how to branch to WebUI or API if needed.                                                                                                                                  | Align with `specs/Public/TUI-PRD.md`.                                                            |
| WebUI                      | Explain task feed, session details layout, and how IDE launches are wired. Include a “minimum permissions” blurb and note when features require REST service connectivity.                                                                                                               | Align with `specs/Public/WebUI-PRD.md`.                                                          |
| AgentFS                    | Overview, mounting instructions, examples for reading/writing, and common error handling.                                                                                                                                                                                                | Pull from AgentFS runbooks/specs once available.                                                 |
| Recorder                   | What is captured, how to enable/disable, how to export or scrub.                                                                                                                                                                                                                         | Keep privacy notes prominent.                                                                    |
| API                        | Concepts, authentication, example requests, and links to generated reference if present.                                                                                                                                                                                                 | Prefer runnable `curl`/HTTP examples.                                                            |
| Development / Contributing | Running docs locally, repo layout, how to add/edit pages, style guide, and how to write tests. Include a small “Docs quality checklist” (links, headings, code fences).                                                                                                                  | Link to `docs/AI-Development-Guide.md` and contributor guides.                                   |

Navigation ordering is guided by filesystem layout (`app/**/page.mdx`) and optional `_meta.json` files for label/order overrides; keep `_meta.json` close to content so contributors can update navigation with the page changes.

### Layout and Navigation

The docs site uses the **Nextra Docs layout**, keeping default behavior for header, sidebar, table of contents, theme toggle, responsive drawer, and navigation. Customizations focus on branding, navigation structure, and optional version text.

- **Persistent header**:
  - Project name / logo for Agent Harbor.
  - Link to the GitHub repository (wired via `docsRepositoryBase` to this repo, not the Nextra example repo).
  - Search trigger using Nextra’s default search UI.
  - Optional static version indicator or dropdown linking to active/archived docs deployments; omit if not configured.

- **Sidebar navigation**:
  - Collapsible groups for the top-level sections above.
  - Highlighted active page using Nextra defaults.
  - Labels sourced from MDX or `_meta.json` where provided.
  - Logical ordering (Intro → Quick Start → TUI/WebUI/etc. → Development) with stable anchors so links remain durable.
  - Theme toggle (light/dark/system) via the Nextra default control.

- **Main content area**:
  - Renders a single documentation page at a time.
  - Shows “Previous” / “Next” navigation at the bottom where appropriate (Nextra default).
  - In-page table of contents on the right for headings `##` and below (Nextra default).
  - Optional banner slot above the navbar for time-sensitive notices (beta warnings, migration alerts).

- **Responsive behavior** (Nextra default):
  - On small screens, the sidebar collapses into a drawer.
  - Navigation remains usable on mobile without horizontal scrolling.

Keyboard behavior:

- Users can navigate header, search, sidebar, and content using Tab / Shift+Tab.
- Focus states are visible and predictable.

### Search

The docs site provides **built-in full-text search** over all published documentation:

- Search is accessible from the header or top of the sidebar.
- Results show:
  - Page title.
  - A short snippet with the query highlighted.
- Keyboard-friendly:
  - Typing in the search input and using arrow keys moves between results.
  - Enter navigates to the selected result.

Search operates entirely on **static data** (no external SaaS dependency) so that:

- Previews and offline builds work.
- Deployments remain portable across hosting providers.
- Static indexes are generated with Pagefind (`yarn build:pagefind`), emitted into `public/_pagefind`, and packaged with the static export so search works on any CDN without server functions.

### Content Source and Authoring Model

- The **source of truth** for docs is markdown/MDX files in the repository. Author pages under `apps/docs.agent-harbor.dev/app/**` (App Router) and keep canonical content in the existing in-repo docs (`/docs`, `/specs/Public`) referenced via includes/links rather than duplication.
- The docs site **imports and renders** those files; there is no manual copy/paste of content into a separate system.

Requirements:

- Each page:
  - Begins with a short summary paragraph describing its purpose.
  - Declares `title`/`description` where supported by Nextra for better SEO/sidebar labels.
  - Uses consistent heading hierarchy (`#`, `##`, `###`).
  - Uses fenced code blocks with language hints (` ```bash`, ` ```rust`, etc.) where relevant and includes runnable snippets when possible.
  - Incorporates Nextra MDX components for clarity:
    - Callouts (Note/Tip/Warning) for caveats and migrations.
    - `<Steps>` for ordered procedures.
    - Tabs/CodeTabs for platform differences (Linux/macOS/Windows), keeping command text DRY.
    - Tables for matrices (feature availability, role permissions, browser support).
- Internal links use docs-relative URLs, not raw GitHub URLs, so navigation stays within the docs site.
- When linking to specs/PRDs, prefer short contextual summaries instead of copying entire sections.
- There is a **“Contributing to Documentation”** page explaining:
  - Where docs live in the repo.
  - How to run the local docs dev server.
  - How to add new pages and update navigation.
  - Style and tone guidelines (short intros, examples, consistent headings).
  - Expectations for screenshots/media (alt text, light/dark pairs if necessary).

### Versioning and Releases (Lightweight)

The docs site stays aligned with releases while using Nextra’s default single-version model:

- The **main docs** present the current recommended behavior (matching `main` / latest stable).
- Older documentation is accessible via **clearly labeled links**, for example:
  - A small “Previous versions” area linking to older deployed docs instances.
  - Version labels that correspond to binary / release versions (e.g., “v0.4”, “v0.5”).
- Use the **Nextra versions feature** to surface the active version and provide a version selector that links to prior static builds.
- Previous-version links should point to **permalinked Cloudflare Pages deployments** for those releases, keeping legacy docs reachable without rebuilding new artifacts.

Publishing processes should cover both:

- New major versions of Agent Harbor, keeping prior docs accessible via permalinks for each released version.
- Corrections and amendments to existing pages outside a versioned release cycle, with clear publication steps and expectations (including how previews are reviewed before promotion).

The PRD does **not** mandate a complex version switcher; it only requires:

- Users can tell which version they are reading.
- Users can find older versions from the docs site itself.

### Internationalization (i18n)

- Use Next.js/Nextra built-in i18n routing (locales defined in `next.config.mjs` and Nextra `i18n` entries) rather than bespoke routing.
- Define and document the URL scheme for localized content (e.g., `/en/`, `/fr/` prefix) to keep language variants predictable and linkable.
- Ensure navigation, search, and version/permalink structures are compatible with the chosen i18n URL pattern.
- Avoid runtime translation dependencies; localized pages should be part of the static export with language toggles or links surfaced in the UI.

### Behavior and Quality Requirements

- **All key topics** (Intro, Quick Start, TUI, WebUI, AgentFS, Recorder, API, Development Guides) are represented as pages.
- **Internal cross-links**:
  - Quick Start links to TUI/WebUI where appropriate.
  - TUI/WebUI pages link back to Quick Start and relevant API docs.
  - AgentFS/Recorder pages link to configuration/usage examples.
- **Broken links and obvious formatting issues** are caught before publication via automated checks in the standard development workflow (exact tools are implementation detail).
- The docs site must build as a **static site** suitable for hosting on a generic static hosting provider.

### Deployment and Hosting

- Builds must produce a fully static export (`yarn workspace site build` → `apps/docs.agent-harbor.dev/out`) using the Next.js `output: 'export'` setting in `apps/docs.agent-harbor.dev/next.config.mjs`; this artifact is the sole deployable for previews and production.
- Avoid server-rendered routes, API routes, or runtime data fetches that require Node.js/Edge execution; everything (including the built-in Pagefind search assets) ships in the static export so the site remains CDN-friendly.
- Treat missing or non-static `apps/docs.agent-harbor.dev/out` output as a build failure so previews and production deploys cannot regress to dynamic hosting requirements.
- Hosting uses **Cloudflare Pages** with two flows:
  - **Main branch** deployments publish the static export (`apps/docs.agent-harbor.dev/out`) to the primary docs URL.
  - **Pull request previews** upload the same static export to a preview URL tied to the branch/PR for review.
- Deployment automation must rely on the static artifact only: the pipeline should fail if the export is absent or contains non-static features, and it should surface the resulting Pages URL (production or preview) for reviewers.

### Integration with the Main Repository

- The repository **README** contains a prominent link (or badge) to the docs site.
- Internal architecture docs (such as component architecture overviews) reference the docs site where appropriate, and vice versa.
- There is a clear **“Docs entrypoint” URL** that is stable and suitable for use in:
  - README.
  - Release notes.
  - External references.

### Visual Design and Theming

- The docs site follows the **Agent Harbor branding**:
  - Uses Agent Harbor name and logo in the header.
  - Uses a color palette that feels consistent with the TUI/WebUI (without needing pixel-perfect matches).
- The site supports:
  - **Light and dark themes**, selectable by the user and/or following system preference, using the default Nextra theme toggle.
- Typography and layout requirements:
  - Clear visual distinction between headings, body text, and code.
  - Comfortable line length and spacing for long-form reading.
  - Code blocks and inline code are easily distinguishable.
- Use Nextra’s standard components (callouts/notes/warnings) for:
  - Important caveats.
  - Warnings and migration notes.
  - Tips and best practices.

### Accessibility

The docs site should be accessible to a wide range of users:

- **Keyboard-accessible** navigation and search.
- **Visible focus** indicators on interactive elements.
- Use semantic HTML structure:
  - Proper heading levels.
  - Landmarks (main, nav, header).
- Color usage should respect **basic contrast requirements** so text remains readable in both light and dark themes.

### Minimal Functional Acceptance Criteria

From a user’s point of view, the Docs Site is “done” when:

1. There is a **stable public URL** for the docs site.
2. The **homepage**:
   - Explains what Agent Harbor is.
   - Offers a clear Quick Start entry.
3. The **navigation** exposes at least:
   - Intro / Overview
   - Quick Start
   - TUI
   - WebUI
   - AgentFS
   - Recorder
   - API
   - Development / Contributing
4. **Search** works and can find pages by key terms (e.g., “TUI”, “AgentFS”, “local mode”).
5. There is a **documented contributor workflow** for editing and adding docs.
6. The repository README links to the docs, and the docs site can be used as the primary reference for Agent Harbor features and workflows.
