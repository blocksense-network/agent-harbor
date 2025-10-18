## Crate Architecture and Dependencies

### ah-rest-client Design Principles

The `ah-rest-client` crate is intentionally designed with minimal dependencies to enable third-party usage:

- **No dependency on ah-core**: The REST client crate does not implement the `TaskManager` trait or depend on ah-core, keeping it lightweight
- **Third-party friendly**: External software can use `ah-rest-client` to interact with Agent Harbor APIs without pulling in heavy dependencies like multiplexers, local execution engines, or database abstractions
- **Low-level HTTP focus**: The crate provides direct HTTP client functionality and implements the `ClientApi` trait for ecosystem compatibility
- **Composable architecture**: ah-core uses the REST client to implement `TaskManager`, but the REST client stands alone as a minimal HTTP library

This design allows the Agent Harbor ecosystem to have clean dependency boundaries while enabling flexible composition and third-party integration.
