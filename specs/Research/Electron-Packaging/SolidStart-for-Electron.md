Here’s a battle-tested way to wrap a SolidStart app in Electron, with packaging for macOS/Windows/Linux. I’ll show the simplest (CSR/static) path first, then note how to keep SSR if you really need it.

---

# 1) Make your SolidStart build “renderer-friendly” (CSR/static)

For desktop apps you usually don’t need SSR. Configure SolidStart to emit a static site so Electron can load an `index.html` directly.

**`app.config.ts`**

```ts
import { defineConfig } from "@solidjs/start/config";
import electron from "vite-plugin-electron/simple";

export default defineConfig({
  // Produce static output in .output/public (no server functions/API routes)
  server: { preset: "static" }, // CSR/SSG build
  // Hook into the client router's Vite build and add the Electron plugin
  vite({ router }) {
    if (router === "client") {
      return {
        plugins: [
          electron({
            main:   { entry: "electron/main.ts" },
            preload:{ input: { preload: "electron/preload.ts" } },
            // renderer: {} // optional: enable Node API in renderer via preload
          })
        ]
      };
    }
    return {};
  }
});
```

* `server: { preset: "static" }` tells SolidStart/Vinxi/Nitro to pre-render and put files in `.output/public`. ([Answer Overflow][1])
* Attaching plugins per “router” (here, only the **client** build gets the Electron plugin) is the documented SolidStart way to customize Vite per build. ([docs.solidjs.com][2])
* `vite-plugin-electron/simple` is the straightforward integration—its default dev behavior provides `process.env.VITE_DEV_SERVER_URL` and builds main/preload into `dist-electron` for production. ([GitHub][3])

---

# 2) Add the Electron entry points

Create an `electron/` folder with your main & preload:

**`electron/main.ts`**

```ts
import { app, BrowserWindow } from "electron";
import path from "node:path";

const isDev = !!process.env.VITE_DEV_SERVER_URL;

function createWindow() {
  const win = new BrowserWindow({
    width: 1200,
    height: 800,
    webPreferences: {
      preload: path.join(__dirname, "preload.mjs"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  if (isDev) {
    // During `pnpm dev`, Electron loads the Vite dev server URL
    win.loadURL(process.env.VITE_DEV_SERVER_URL!);
  } else {
    // In production, load the static SolidStart build (see step 3)
    const indexHtml = path.join(process.resourcesPath, ".output/public/index.html");
    win.loadFile(indexHtml);
  }
}

app.whenReady().then(createWindow);
app.on("window-all-closed", () => { if (process.platform !== "darwin") app.quit(); });
```

**`electron/preload.ts`**

```ts
import { contextBridge } from "electron";

contextBridge.exposeInMainWorld("api", {
  ping: () => "pong"
});
```

(The `VITE_DEV_SERVER_URL` + `loadFile()` flow mirrors the plugin’s quick-start.) ([GitHub][3])

---

# 3) Ensure the static files ship with your app

By default, SolidStart’s static output lives in `.output/public`. Include it (and the Electron build output) in your installer via **electron-builder**:

**`electron-builder.json5`**

```json5
{
  "appId": "com.yourco.yourapp",
  "productName": "YourApp",
  "directories": { "output": "release/${version}" },
  "files": [
    ".output/public",        // SolidStart client build (index.html + assets)
    "dist-electron"          // vite-plugin-electron output (main/preload)
  ],
  "win":   { "target": [{ "target": "nsis", "arch": ["x64"] }] },
  "mac":   { "target": ["dmg"] },
  "linux": { "target": ["AppImage"] }
}
```

This mirrors the typical “include renderer + `dist-electron`” pattern used in the Electron⚡️Vite docs—just swap `dist` for `.output/public`. ([electron-vite.github.io][4])

---

# 4) Scripts

**`package.json`** (relevant parts)

```json
{
  "main": "dist-electron/main.mjs",
  "scripts": {
    "dev": "pnpm run dev:app",
    "dev:app": "vinxi dev",             // SolidStart dev; plugin boots Electron
    "build": "vinxi build",             // SolidStart build (.output/public)
    "build:electron": "vite build",     // ensures dist-electron is built
    "pack": "pnpm build && pnpm build:electron && electron-builder"
  },
  "devDependencies": {
    "vite-plugin-electron": "^0.29.0",
    "electron-builder": "^24"
  }
}
```

* Running `pnpm dev` gives HMR for the renderer and auto-restarts main/preload via the plugin. ([GitHub][3])
* `pnpm pack` creates installers using electron-builder’s standard config. ([electron-vite.github.io][4])

---

# 5) Client-side routing & deep links (important)

Because you’re loading files from disk in production, deep-linked routes like `app://…/settings` don’t exist on the filesystem. Use a small helper to always fall back to `index.html`:

* Easiest: add **electron-serve** to serve your static folder with history-fallback, then `win.loadURL(serve.url())`. ([GitHub][5])

(Alternatively, make sure users always land on `index.html` and navigate in-app.)

---

# 6) Keeping SSR (optional/advanced)

If you truly need SolidStart SSR inside Electron:

* Build with the default (node) preset so Nitro outputs a Node server entry at `.output/server/index.mjs`, then spawn it from Electron and point your `BrowserWindow` at `http://127.0.0.1:<port>`. Nitro’s “node_server” preset produces a ready-to-run server entry. ([nitro.build][6])

> Most desktop apps don’t need SSR; the static route is simpler and avoids a background server process.

---

## Common gotchas

* **Where’s my `index.html`?** If you see `.output/server/index.mjs` but no HTML, you’re on an SSR build; switch to a static/CSR preset so files appear in `.output/public`. ([Microsoft Learn][7])
* **Plugin placement:** Add `vite-plugin-electron` only to the **client** router’s Vite config (as shown), not the server builds. ([docs.solidjs.com][2])
* **SolidStart internals:** SolidStart uses **Vinxi** (Vite) for builds and **Nitro** for the production server—handy to know when you see `.output/…` paths. ([docs.solidjs.com][8])

---

If you paste your repo’s scripts/config, I can adapt the exact paths (e.g., if you prefer `dist/` instead of `.output/public`) and give you a drop-in `electron-builder.json5`.

[1]: https://www.answeroverflow.com/m/1231881686808002591?utm_source=chatgpt.com "Build process with solidstart - SolidJS"
[2]: https://docs.solidjs.com/solid-start/reference/config/define-config?utm_source=chatgpt.com "defineConfig - SolidStart Docs"
[3]: https://github.com/electron-vite/vite-plugin-electron "GitHub - electron-vite/vite-plugin-electron: :electron: Electron⚡️Vite core repo"
[4]: https://electron-vite.github.io/build/electron-builder.html "electron-builder | Electron⚡️Vite"
[5]: https://github.com/sindresorhus/electron-serve?utm_source=chatgpt.com "Static file serving for Electron apps - GitHub"
[6]: https://nitro.build/deploy/runtimes/node?utm_source=chatgpt.com "Node.js - Nitro - UnJS"
[7]: https://learn.microsoft.com/en-us/answers/questions/4370675/static-web-app-with-solidstart?utm_source=chatgpt.com "Static Web App with SolidStart? - Microsoft Q&A"
[8]: https://docs.solidjs.com/solid-start/getting-started?utm_source=chatgpt.com "Getting started - SolidStart Docs"
