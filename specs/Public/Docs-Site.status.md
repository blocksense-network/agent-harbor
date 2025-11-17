### Overview

This document tracks the implementation status of the [Docs-Site-PRD.md](Docs-Site-PRD.md) functionality.

Goal: deliver a production-ready documentation site (Next.js 16 + Nextra 4) that renders in-repo Markdown/MDX with fast navigation, offline search (Pagefind), and a low-friction contributor workflow.

Current status: foundation drafted — Nextra scaffold exists (`docs/site/app/layout.jsx`, `mdx-components.js`, `next.config.mjs`); homepage stub present, but navigation, content pages, branding, and contributor flows remain to be built.
Test results: not run (docs site build/search tests not yet wired into CI for this iteration).

### Status Summary

- Structure: Next.js App Router with Nextra Docs layout is present; `Layout` renders `getPageMap` for sidebar generation. Needs repo-specific `docsRepositoryBase`, logo, theme toggle, and banner slot.
- Content: Only `docs/site/app/page.mdx`; no section pages, `_meta.json` ordering, or in-page TOC headings.
- Branding/metadata: Header uses Nextra defaults; `docsRepositoryBase` still points to the Nextra repo; no favicon/logo/title/OG metadata.
- Search/export: Pagefind dependency and script exist, but CI/export do not enforce fresh `_pagefind` artifacts.
- Tooling: Site is JS-only with default Next webpack; Turbopack not configured; no `just` targets, CI jobs, or smoke/e2e tests.
- Authoring workflow: No on-site contributing page; commands and MDX conventions undocumented; edit-link target not wired to this repo.
- Deployment/versioning: Static export in `docs/site/out` from `yarn workspace site build` (Next.js `output: 'export'`) is the sole deployable; Cloudflare Pages should publish main to production and PR branches to previews, with guardrails for SSR-only features. Version surfacing should use Nextra’s built-in versions support where available, and older docs stay reachable via permalinked Cloudflare deployments. Flows for major releases and off-cycle corrections must be documented.

### Plan

- [ ] Brand assets & metadata: add favicon, logo, typography (font loading), HTML title/description/OG metadata, theme-color; wire `docsRepositoryBase` in `docs/site/app/layout.jsx`.
- [ ] Content & navigation: implement `_meta.json` ordering and section pages for Homepage, Quick Start, TUI, WebUI, AgentFS, Recorder, API, Contributing; add TOCs/breadcrumbs where supported and ensure cross-links to existing repo docs/specs; prefer Nextra defaults where they satisfy needs.
- [ ] TypeScript + Turbopack: migrate the docs site to TypeScript (typed MDX where possible) and configure Turbopack for dev; keep compatibility with Nextra 4 App Router and search/index build.
- [ ] Linting, build, tests, and just targets: add `just docs-site-{lint,build,test,dev}`; enforce `next lint`, type-check, Nextra MDX validation, and Pagefind generation in CI. Block merges on lint/build/search/test failures.
- [ ] Testing: add smoke/e2e tests (Playwright or Next built-in) that cover navigation, TOC rendering, search availability, and section routes; ensure tests emit per-run logs per repo guidance.
- [ ] Search pipeline: ensure `just docs-site-build` runs Pagefind, treats stale `_pagefind` artifacts as failures, and publishes search assets with static export.
- [ ] Deployment (Cloudflare Pages + PR previews): add Cloudflare Pages config for static export; integrate GitHub PR previews with comment links to preview URLs and build artifacts.
- [ ] Contributing workflow: publish an on-site authoring page covering `yarn --cwd docs/site dev`, MDX components, style/tone, edit-link config, PR preview expectations, and media guidelines.
- [ ] Version surfacing: use Nextra versions (as supported) to label the active version and link to prior Cloudflare deployment permalinks; ensure “current vs previous” is visible.
- [ ] Observability (optional): emit bundle/search index size metrics in CI logs to spot regressions.
- [ ] i18n: define URL scheme (e.g., `/en/`, `/fr/`), ensure navigation/search/version links work per locale, keep localized pages in the static export without runtime translation dependencies, and rely on Nextra/Next i18n defaults where sufficient.

### Key Source Files (current scaffold)

- `docs/site/app/layout.jsx`: wraps Nextra `Layout`/`Navbar` and `getPageMap`; placeholder `docsRepositoryBase` still references the Nextra repo.
- `docs/site/app/page.mdx`: homepage stub; should become hero + tiles per PRD “Homepage” inventory item.
- `docs/site/mdx-components.js`: merges Nextra theme components with custom MDX components; entry point for adding custom shortcodes.
- `docs/site/next.config.mjs`: Nextra wrapper; future home for Turbopack/TS enabling and static export tweaks.
- `docs/site/package.json`: scripts for `dev`, `build`, and `build:pagefind`; Pagefind dependency already present.
- `docs/site/public/_pagefind/*`: prior local build artifacts; should be regenerated in CI.
- `docs/site/app/globals.css`: global styling baseline; will need branding tokens when palette/typography are defined.

### Additional Work Items

- Add `.turbo`/Turbopack config and TypeScript `tsconfig` plus MDX type support.
- Add `_meta.json` files per section to enforce navigation order and labels.
- Configure `just` targets to run in Nix shell; mirror test-writing guidance (per-test logs) for docs tests.
- Add Cloudflare Pages/project config file and GitHub workflow for PR previews with linked artifacts.
- Add linting rules for MDX/links (broken-link checker) and ensure Pagefind runs post-export.
- Add a “Docs dev setup” doc page that links to repo README and AI development guide for style/quality expectations.
