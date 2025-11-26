# REST Service tech stack

**TLS/SSL:** **rustls**

- rustls is the most mature Rust-only SSL/TLS crate. It's a modern, low-level library that implements the TLS protocol entirely in Rust, emphasizing security, ease of use, and no unsafe code. It supports TLS 1.2 and 1.3, various cipher suites, and integrates well with async runtimes like Tokio via tokio-rustls.

**HTTP framework:** **Axum** (on Tokio/Hyper/Tower)

- Modern, modular, works with Tower middleware (timeouts, tracing, CORS, compression, rate limiting). Axum treats middleware as Tower layers, which is a big long-term win for composability. ([Docs.rs][1])

**CORS & common middleware:** **tower-http**

- Use `CorsLayer`, `TraceLayer`, compression, request IDs, etc. CORS is first-class and documented. ([Docs.rs][2])

**Server-Sent Events (live updates):** **axum::response::sse**

- Built-in SSE type (`Sse`, `Event`, `KeepAlive`) with examples in the official docs. No extra crate needed. ([Docs.rs][3])

**OpenAPI / Swagger UI:** **utoipa + utoipa-axum + utoipa-swagger-ui (or RapiDoc)**

- Code-first OpenAPI generation with tight Axum bindings; serve Swagger UI or RapiDoc directly. The project is active with frequent releases. ([GitHub][4])

**Auth (JWT):** **jsonwebtoken**

- Mature JWT encode/decode/validate with JWK support. Use with `rust_crypto` feature to use pure Rust crypto implementations instead of OpenSSL. ([Docs.rs][5])

**Rate limiting:** **tower::limit::RateLimitLayer** (simple global) or **tower_governor** (IP/API-key aware)

- Start with Tower’s built-in rate limiter; if you need keyed quotas/burst control, `tower_governor` layers cleanly on Tower/Axum. ([Tower RS][6])

**Observability:** **tracing + tracing-subscriber + OpenTelemetry**

- Axum/Tower integrate with `tracing`; export spans/metrics via OpenTelemetry crates. (OTel docs recommend using `tracing` macros and bridging via opentelemetry exporters.) ([OpenTelemetry][7])

**Database:** **SQLx** (or **rusqlite** for simpler deployments)

- Async, runtime-agnostic, compile-time checked queries; supports Pg/MySQL/SQLite; widely adopted and maintained. For simpler deployments, rusqlite provides synchronous SQLite access. ([GitHub][8])

**Note:** By selecting rustls-based features on crates (e.g., `jsonwebtoken = { version = "10", default-features = false, features = ["rust_crypto"] }`, `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }`), you can eliminate OpenSSL dependencies entirely and use pure-Rust TLS implementations.

**HTTP client (for integration & E2E tests):** **reqwest**

- De-facto async client; use in tests and any outbound calls. Enable `rustls-tls` feature instead of default `native-tls` to use rustls instead of OpenSSL. ([Docs.rs][9])

**ACP gateway (JSON-RPC over WebSocket/stdio):** Feature-gated `acp` section in the `ah-rest-server` config. Defaults keep the gateway **disabled** (`enabled = false`) with a local-only WebSocket listener at `127.0.0.1:3031`, `transport = websocket`, and `auth_policy = inherit-rest` (reuses API key/JWT validation). Opt into `transport = stdio` for `ah agent access-point` sidecars; `auth_policy = anonymous` is reserved for air-gapped local development. A short primer and milestone tracker live in `specs/ACP.server.status.md`.

When enabled, the gateway currently supports ACP `initialize`, `session/new`, `session/list`, and `session/load` RPCs mapped onto the existing REST session primitives. Lifecycle events are mirrored as `session/update` notifications using the same `SessionStore` event bus that powers REST SSE.

Example configuration block:

```
[acp]
enabled = false
bind_addr = "127.0.0.1:3031"
transport = "websocket"   # or "stdio"
auth_policy = "inherit-rest"  # or "anonymous" (local-only)
# Connection guards
connection_limit = 32         # max concurrent ACP clients
idle_timeout_secs = 30        # close idle sockets defensively
```

**Black-box HTTP mocking (tests):** **wiremock**

- Parallel-safe in-process mock server; works great with reqwest/async. ([Docs.rs][10])

[1]: https://docs.rs/axum/latest/axum/middleware/index.html?utm_source=chatgpt.com 'axum::middleware - Rust'
[2]: https://docs.rs/tower-http/latest/tower_http/cors/struct.CorsLayer.html?utm_source=chatgpt.com 'CorsLayer in tower_http::cors - Rust'
[3]: https://docs.rs/axum/latest/axum/response/sse/?utm_source=chatgpt.com 'axum::response::sse - Rust'
[4]: https://github.com/juhaku/utoipa?utm_source=chatgpt.com 'GitHub - juhaku/utoipa: Simple, Fast, Code first and Compile time generated OpenAPI documentation for Rust'
[5]: https://docs.rs/jsonwebtoken?utm_source=chatgpt.com 'jsonwebtoken - Rust'
[6]: https://tower-rs.github.io/tower/tower/limit/rate/struct.RateLimitLayer.html?utm_source=chatgpt.com 'RateLimitLayer in tower::limit::rate - Rust'
[7]: https://opentelemetry.io/docs/languages/rust/?utm_source=chatgpt.com 'Rust | OpenTelemetry'
[8]: https://github.com/launchbadge/sqlx?utm_source=chatgpt.com 'GitHub - launchbadge/sqlx: 🧰 The Rust SQL Toolkit. An async, pure Rust SQL crate featuring compile-time checked queries without a DSL. Supports PostgreSQL, MySQL, and SQLite.'
[9]: https://docs.rs/reqwest/latest/reqwest/blocking/?utm_source=chatgpt.com 'reqwest::blocking - Rust'
[10]: https://docs.rs/wiremock/?utm_source=chatgpt.com 'wiremock - Rust'
