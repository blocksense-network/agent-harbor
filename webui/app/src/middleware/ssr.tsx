// Client-side rendering with server-side HTML shell
import * as fs from "fs";
import * as path from "path";

// Minimal Express response interface for our middleware needs
interface ExpressResponse {
  status(code: number): {
    json: (data: unknown) => void;
    set: (header: string, value: string) => { send: (html: string) => void };
  };
}

// Simple logger that respects quiet mode for testing
const logger = {
  log: (...args: unknown[]) => {
    const isQuietMode =
      process.env["QUIET_MODE"] === "true" ||
      process.env["NODE_ENV"] === "test";
    if (!isQuietMode) {
      console.log(...args);
    }
  },
  warn: (...args: unknown[]) => {
    const isQuietMode =
      process.env["QUIET_MODE"] === "true" ||
      process.env["NODE_ENV"] === "test";
    if (!isQuietMode) {
      console.warn(...args);
    }
  },
  error: (...args: unknown[]) => {
    console.error(...args); // Always log errors
  },
};

// Function to find the correct CSS and JS filenames
const getAssetFilenames = (): { css: string; js: string } => {
  const publicDir = path.resolve("./dist/public");

  try {
    const files = fs.readdirSync(publicDir);
    const cssFile = files.find((f) => f.endsWith(".css"));
    const jsFile = files.find((f) => f.endsWith(".js"));

    return {
      css: cssFile || "client.css",
      js: jsFile || "client.js",
    };
  } catch (error) {
    logger.warn(
      "Could not read public directory, using default filenames:",
      error,
    );
    // Hardcode the known filenames for now
    return {
      css: "client-CwxY70Iu.css",
      js: "client.js",
    };
  }
};

export const ssrMiddleware = async (
  req: unknown,
  res: unknown,
  next: unknown,
) => {
  const request = req as { url: string };
  const response = res as ExpressResponse;
  const nextFn = next as (error?: unknown) => void;

  logger.log("SSR middleware called for:", request.url);
  try {
    // Skip SSR for API routes and static assets
    if (request.url.startsWith("/api") || request.url.includes(".")) {
      logger.log("Skipping SSR for:", request.url);
      return nextFn();
    }

    // Only serve HTML for known application routes, otherwise let Express handle 404
    const knownRoutes = ["/"];
    const isKnownRoute = knownRoutes.some((route) => {
      if (route === "/") return request.url === "/";
      return false;
    });

    if (!isKnownRoute) {
      // For API requests, return JSON error; for others, let Express handle
      if (request.url?.startsWith("/api")) {
        response.status(404).json({
          type: "https://docs.example.com/errors/not-found",
          title: "Not Found",
          status: 404,
          detail: `The requested resource '${request.url}' was not found`,
        });
        return;
      }
      return nextFn(); // Let Express handle non-API 404s
    }

    // Find the correct asset filenames
    const { css: cssFilename, js: jsFilename } = getAssetFilenames();

    // Serve HTML shell that will be hydrated by client-side JavaScript
    const appHtml = `
      <div id="app"><div></div></div>
    `;

    // HTML template with server-rendered content for hydration
    const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Harbor</title>
  <link rel="icon" type="image/svg+xml" href="/favicon.ico">
  <meta name="description" content="Web-based dashboard for creating, monitoring, and managing agent coding sessions">
  <link rel="stylesheet" href="/assets/${cssFilename}">
  <style>
    /* Critical CSS for initial render */
    body {
      margin: 0;
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif;
      background-color: #f8fafc;
      color: #1e293b;
    }
    .h-screen {
      height: 100vh;
    }
    .flex {
      display: flex;
    }
    .flex-col {
      flex-direction: column;
    }
    .bg-gray-50 {
      background-color: #f9fafb;
    }
  </style>
</head>
<body>
  <div id="app">${appHtml}</div>

  <!-- Fallback for when JavaScript is disabled -->
  <noscript>
    <div style="padding: 2rem; text-align: center; background: #fef2f2; border: 1px solid #fca5a5; margin: 1rem; border-radius: 0.5rem;">
      <h2 style="color: #dc2626; margin: 0 0 1rem 0;">JavaScript Required</h2>
      <p style="margin: 0; color: #7f1d1d;">
        This application requires JavaScript to function properly.
        Please enable JavaScript in your browser settings and reload the page.
      </p>
      <p style="margin: 1rem 0 0 0; color: #7f1d1d;">
        <a href="/" style="color: #dc2626; text-decoration: underline;">Reload Page</a>
      </p>
    </div>
  </noscript>

  <!-- Load the client-side JavaScript bundle -->
  <script type="module" src="/assets/${jsFilename}"></script>
</body>
</html>`;

    response.status(200).set("Content-Type", "text/html").send(html);
  } catch (error) {
    logger.error("SSR Error:", error);
    nextFn(error);
  }
};
