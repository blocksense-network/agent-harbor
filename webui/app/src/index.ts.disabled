import express from "express";
import cors from "cors";
import helmet from "helmet";
import morgan from "morgan";
// Note: These imports are for future SSR implementation
// import { createProxyMiddleware } from "http-proxy-middleware";
// import { renderToString } from "solid-js/web";
// import { generateHydrationScript } from "solid-js/web";
// import App from "./App";
import { apiProxyMiddleware } from "./middleware/apiProxy.js";
import { ssrMiddleware } from "./middleware/ssr.js";

const app = express();
const PORT = process.env.PORT || 3000;
const NODE_ENV = process.env.NODE_ENV || "development";

// Middleware
app.use(
  helmet({
    contentSecurityPolicy: {
      directives: {
        defaultSrc: ["'self'"],
        styleSrc: ["'self'", "'unsafe-inline'"],
        scriptSrc: ["'self'"],
        imgSrc: ["'self'", "data:", "https:"],
        connectSrc: ["'self'", "ws:", "wss:"],
      },
    },
  }),
);

app.use(
  cors({
    origin:
      process.env.NODE_ENV === "production"
        ? process.env.ALLOWED_ORIGINS?.split(",") || false
        : true,
    credentials: true,
  }),
);

app.use(morgan("combined"));
app.use(express.json());

// Health check
app.get("/health", (req, res) => {
  res.json({
    status: "ok",
    timestamp: new Date().toISOString(),
    environment: NODE_ENV,
  });
});

// API proxy middleware - forwards requests to Rust REST service or mock server
app.use("/api", apiProxyMiddleware);

// Static assets from the built client bundle
app.use(express.static("./dist/public"));

// SSR middleware for all other routes
app.use("*", ssrMiddleware);

// Error handler
app.use(
  (
    err: any,
    req: express.Request,
    res: express.Response,
    _next: express.NextFunction,
  ) => {
    console.error(err.stack);
    res.status(500).json({
      type: "https://docs.example.com/errors/internal-server-error",
      title: "Internal Server Error",
      status: 500,
      detail: "An unexpected error occurred",
    });
  },
);

app.listen(PORT, () => {
  console.log(`SSR Sidecar server running on http://localhost:${PORT}`);
  console.log(`Health check: http://localhost:${PORT}/health`);
  console.log(`Environment: ${NODE_ENV}`);
  if (NODE_ENV === "development") {
    console.log(`Mock server proxy: http://localhost:${PORT}/api`);
  }
});
