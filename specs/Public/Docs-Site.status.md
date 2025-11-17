### Overview

This document tracks the implementation status of the [Docs-Site-PRD.md](Docs-Site-PRD.md) functionality.

Goal: deliver a production-ready documentation site (Next.js 15 + Nextra 4) that renders in-repo Markdown/MDX with fast navigation, offline search (Pagefind), and a low-friction contributor workflow.

Current status: foundation drafted — Nextra scaffold exists (`docs/site/app/layout.tsx`, `mdx-components.tsx`, `next.config.mjs`); TypeScript migration complete; homepage stub present, but navigation, content pages, branding, and contributor flows remain to be built.
Test results: not run (docs site build/search tests not yet wired into CI for this iteration).

### Status Summary

- Structure: Next.js App Router with Nextra Docs layout is present; `Layout` renders `getPageMap` for sidebar generation. Migrated to TypeScript (`layout.tsx`). Needs repo-specific `docsRepositoryBase`, logo, theme toggle, and banner slot.
- Content: Only `docs/site/app/page.mdx`; no section pages, `_meta.json` ordering, or in-page TOC headings.
- Branding/metadata: Header uses Nextra defaults; `docsRepositoryBase` still points to the Nextra repo; no favicon/logo/title/OG metadata.
- Search/export: Pagefind dependency and script exist; build script copies `_pagefind` to `out/` directory. CI/export should enforce fresh `_pagefind` artifacts.
- Tooling: Site migrated to TypeScript (`tsconfig.json`, `next-env.d.ts`); ESLint config added. Uses default Next webpack (Turbopack not pursued due to PnP incompatibility); no `just` targets, CI jobs, or smoke/e2e tests.
- Authoring workflow: No on-site contributing page; commands and MDX conventions undocumented; edit-link target not wired to this repo.
- Deployment/versioning: Static export in `docs/site/out` from `yarn workspace site build` (Next.js `output: 'export'`) is the sole deployable; CI deployment to Cloudflare Pages is configured via `deploy-sites` workflow (deploys on PRs and main branch with PR preview comments). Version surfacing should use Nextra's built-in versions support where available, and older docs stay reachable via permalinked Cloudflare deployments. Flows for major releases and off-cycle corrections must be documented.

### Plan

- [ ] Brand assets & metadata: add favicon, logo, typography (font loading), HTML title/description/OG metadata, theme-color; wire `docsRepositoryBase` in `docs/site/app/layout.tsx`.
- [ ] Content & navigation: implement `_meta.json` ordering and section pages for Homepage, Quick Start, TUI, WebUI, AgentFS, Recorder, API, Contributing; add TOCs/breadcrumbs where supported and ensure cross-links to existing repo docs/specs; prefer Nextra defaults where they satisfy needs.
- [x] Migrate the docs site to TypeScript (typed MDX where possible); keep compatibility with Nextra 4 App Router and search/index build.
- [x] Linting (ESLint + Prettier): ESLint and Prettier are configured; pre-commit hooks enforce formatting and linting on all files including docs site. ESLint includes TypeScript resolver for `docs/*/tsconfig*.json`.
- [ ] Add `just docs-site-{dev,build,lint,test}` targets: configure `just` targets to run docs site commands in the Nix shell: `docs-site-dev` (dev server), `docs-site-build` (build with Pagefind), `docs-site-lint` (linting and type-check), `docs-site-test` (tests with per-test logs per repo guidance).
- [ ] Testing: add smoke/e2e tests (Playwright or Next built-in) that cover navigation, TOC rendering, search availability, and section routes; ensure tests emit per-run logs per repo guidance.
- [x] Search pipeline: ensure `just docs-site-build` runs Pagefind, treats stale `_pagefind` artifacts as failures, and publishes search assets with static export.
- [x] Deployment (Cloudflare Pages + PR previews): CI deployment configured via `.github/workflows/deploy-site.yml` reusable workflow; `deploy-sites` job in `ci.yml` deploys docs site to Cloudflare Pages with PR preview comments. Deploys on PRs (preview) and main branch (production) with commit hash and branch tracking.
- [ ] Version surfacing: use Nextra versions (as supported) to label the active version and link to prior Cloudflare deployment permalinks; ensure “current vs previous” is visible.
- [ ] i18n: define URL scheme (e.g., `/en/`, `/fr/`), ensure navigation/search/version links work per locale, keep localized pages in the static export without runtime translation dependencies, and rely on Nextra/Next i18n defaults where sufficient.

### Key Source Files (current scaffold)

- `docs/site/app/layout.tsx`: wraps Nextra `Layout`/`Navbar` and `getPageMap`; placeholder `docsRepositoryBase` still references the Nextra repo. Migrated to TypeScript.
- `docs/site/app/page.mdx`: homepage stub; should become hero + tiles per PRD "Homepage" inventory item.
- `docs/site/mdx-components.tsx`: merges Nextra theme components with custom MDX components; entry point for adding custom shortcodes. Migrated to TypeScript.
- `docs/site/next.config.mjs`: Nextra wrapper; static export configuration.
- `docs/site/package.json`: scripts for `dev`, `build`, and `build:pagefind`; Pagefind dependency already present. Build script copies `_pagefind` to `out/`.
- `docs/site/public/_pagefind/*`: prior local build artifacts; should be regenerated in CI.
- `docs/site/app/globals.css`: global styling baseline; will need branding tokens when palette/typography are defined.
- `docs/site/tsconfig.json`: TypeScript configuration for the docs site.
- `docs/site/next-env.d.ts`: Next.js TypeScript environment declarations.

### Additional Work Items

- Add `_meta.json` files per section to enforce navigation order and labels.
- Add linting rules for MDX/links (broken-link checker) and ensure Pagefind runs post-export (ESLint and Prettier configured, but MDX-specific rules may need enhancement).
- Add a "Docs dev setup" doc page that links to repo README and AI development guide for style/quality expectations.
