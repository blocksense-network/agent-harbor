Solid is really good at Server-Side Rendering. Here’s a practical way to get **server-rendered HTML that works with JS off**, and then **hydrates/enhances when JS is on**.

# Option A (recommended): SolidStart, SSR + progressive enhancement

SolidStart is Solid’s full-stack meta-framework. It gives you SSR (including streaming), routing, data loaders, and **forms/actions that degrade to plain HTML**. ([docs.solidjs.com][1])

## 1) Scaffold a project

```bash
npm create solid@latest
# choose: “Yes” for SolidStart, pick a template (e.g. with-tailwindcss), TS optional
cd your-app
npm install
npm run dev
```

The official quick start walks you through the prompts. ([docs.solidjs.com][2])

### 2) Routes render on the server first

Create a route like `src/routes/index.tsx`:

```tsx
// src/routes/index.tsx
import { Title } from '@solidjs/meta';
export default function Home() {
  return (
    <>
      <Title>Welcome</Title>
      <h1>Server rendered heading</h1>
      <p>This works with JavaScript disabled.</p>
      <a href="/contact">Contact us</a>
    </>
  );
}
```

SolidStart does SSR by default; the page is HTML first, then hydrates on the client. It supports sync/async/streaming SSR. ([docs.solidjs.com][1])

### 3) Progressive forms with Actions (no-JS → normal POST; JS → enhanced)

```tsx
// src/routes/contact.tsx
import { action, Form } from '@solidjs/router';

const sendMessage = action(async (formData: FormData) => {
  // server-side work
  const name = String(formData.get('name') ?? '');
  // ... send email, save to DB, etc.
  return { ok: true, name };
});

export default function Contact() {
  const [state, { Form: ContactForm }] = sendMessage.use();

  return (
    <>
      <h1>Contact</h1>

      {/* With JS disabled, this is a normal HTML form POST to the action’s URL.
          With JS enabled, it's intercepted and progressively enhanced. */}
      <ContactForm method="post">
        <label>
          Your name
          <input name="name" required />
        </label>
        <button type="submit" disabled={state.pending}>
          {state.pending ? 'Sending…' : 'Send'}
        </button>
      </ContactForm>

      {state.result?.ok && <p>Thanks, {state.result.name}!</p>}
    </>
  );
}
```

Solid Router’s **Actions** integrate with `<Form>` for progressively enhanced mutations: HTML works standalone; with JS, you get optimistic states, pending UI, and no full-page reloads. ([docs.solidjs.com][3])

### 4) Sprinkle interactivity as “islands”

Anything interactive can just be a component that only ships JS for what’s interactive; the rest remains plain HTML. SolidStart leans on Solid’s fast hydration and streaming; for parts that should **never** hydrate (purely static), wrap them with `<NoHydration>`. ([DeepWiki][4])

### 5) Deploy

SolidStart has presets for Vercel, Netlify, Cloudflare, AWS, etc. Pick one and deploy; Cloudflare’s guide is a good reference. ([docs.solidjs.com][1])

# Option B: Roll your own SSR (Vite/Node) and hydrate

If you don’t want SolidStart, you can wire up SSR yourself using Solid’s rendering APIs.

**Server (Node/Express)**

```ts
// server.ts
import express from "express";
import { renderToString } from "solid-js/web";
import App from "./src/App";
import { HydrationScript } from "solid-js/web";

const app = express();
app.use("/assets", express.static("dist/client")); // built client assets

app.get("*", (_req, res) => {
  const body = renderToString(() => <App />);
  // include HydrationScript so the client can hydrate later
  const hydration = renderToString(() => <HydrationScript />);
  res.status(200).send(`<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    ${hydration}
  </head>
  <body>
    <div id="app">${body}</div>
    <script type="module" src="/assets/client-entry.js"></script>
  </body>
</html>`);
});

app.listen(3000);
```

**Client entry (only runs when JS is on)**

```ts
// src/client-entry.tsx
import { hydrate } from "solid-js/web";
import App from "./App";
hydrate(() => <App />, document.getElementById("app")!);
```

This yields **fully usable HTML** with JS off, and **hydration** when JS loads. Use `renderToString`, or `renderToStream` for streaming. Don’t forget to emit `HydrationScript` (or `generateHydrationScript`) in your HTML head. ([docs.solidjs.com][5])

**Tips for a robust DIY setup**

- **Streaming SSR**: swap in `renderToStream` for faster time-to-first-byte on slow pages. ([solidjs.cn][6])
- **Event replay**: `renderToString` can capture events (`eventNames`) before hydration and replay them, smoothing UX if users click fast. ([docs.solidjs.com][5])
- **Opt out of hydration** for static parts with `<NoHydration>`. ([docs.solidjs.com][7])

# Progressive enhancement checklist (Solid flavor)

- **Links**: use plain `<a href="/route">` — works without JS. When JS is on, Solid Router intercepts for client-side transitions.
- **Forms**: prefer `<Form>` + `action()` from `@solidjs/router` so POSTs work with JS off; JS on adds pending states and mutation caching. ([docs.solidjs.com][3])
- **Avoid client-only rendering for essentials**: render all crucial content on the server; enhance later with small interactive components.
- **Scope hydration**: wrap big static areas in `<NoHydration>` to keep the JS budget small. ([docs.solidjs.com][7])
- **Streaming + Suspense**: use streaming SSR with `<Suspense>` for data-heavy sections to show shells immediately. ([docs.solidjs.com][1])

[1]: https://docs.solidjs.com/solid-start?utm_source=chatgpt.com 'SolidStart Docs'
[2]: https://docs.solidjs.com/quick-start?utm_source=chatgpt.com 'Quick start - Solid Docs'
[3]: https://docs.solidjs.com/solid-router/concepts/actions?utm_source=chatgpt.com 'Actions - Solid Router Docs'
[4]: https://deepwiki.com/solidjs/solid-start/2.2-rendering-modes?utm_source=chatgpt.com 'Rendering Modes | solidjs/solid-start | DeepWiki'
[5]: https://docs.solidjs.com/reference/rendering/render-to-string?utm_source=chatgpt.com 'renderToString - Solid Docs'
[6]: https://www.solidjs.com/guides/server 'SolidJS'
[7]: https://docs.solidjs.com/reference/components/no-hydration?utm_source=chatgpt.com '<NoHydration> - Solid Docs - docs.solidjs.com'
