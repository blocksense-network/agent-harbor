// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Authentication methods for the REST API client

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

/// Authentication methods supported by the API
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// API Key authentication (`Authorization: ApiKey <token>`)
    ApiKey(String),
    /// OIDC/JWT Bearer token (`Authorization: Bearer <jwt>`)
    Bearer(String),
    /// No authentication
    None,
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::None
    }
}

impl AuthMethod {
    /// Apply authentication headers to a request
    pub fn apply_to_headers(
        &self,
        headers: &mut HeaderMap,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            AuthMethod::ApiKey(token) => {
                let value = format!("ApiKey {}", token);
                headers.insert(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_str(&value)?,
                );
            }
            AuthMethod::Bearer(token) => {
                let value = format!("Bearer {}", token);
                headers.insert(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_str(&value)?,
                );
            }
            AuthMethod::None => {
                // No headers to add
            }
        }
        Ok(())
    }

    /// Create API key authentication from token string
    pub fn api_key(token: impl Into<String>) -> Self {
        Self::ApiKey(token.into())
    }

    /// Create bearer token authentication from JWT string
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer(token.into())
    }
}

/// Authentication configuration for the client
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub method: AuthMethod,
    pub tenant_id: Option<String>,
}

impl AuthConfig {
    /// Create a new auth config with API key authentication
    pub fn with_api_key(token: impl Into<String>) -> Self {
        Self {
            method: AuthMethod::api_key(token),
            tenant_id: None,
        }
    }

    /// Create a new auth config with bearer token authentication
    pub fn with_bearer(token: impl Into<String>) -> Self {
        Self {
            method: AuthMethod::bearer(token),
            tenant_id: None,
        }
    }

    /// Set the tenant ID for multi-tenant requests
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Get headers for this authentication configuration
    pub fn headers(&self) -> Result<HeaderMap, Box<dyn std::error::Error + Send + Sync>> {
        let mut headers = HeaderMap::new();

        // Apply authentication method
        self.method.apply_to_headers(&mut headers)?;

        // Add tenant header if specified
        if let Some(tenant_id) = &self.tenant_id {
            headers.insert(
                HeaderName::from_static("x-tenant-id"),
                HeaderValue::from_str(tenant_id)?,
            );
        }

        Ok(headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ah_test_utils::logged_test]
    fn test_api_key_auth_headers() {
        let auth = AuthMethod::api_key("test-token");
        let mut headers = HeaderMap::new();
        auth.apply_to_headers(&mut headers).unwrap();

        assert_eq!(headers.get("authorization").unwrap(), "ApiKey test-token");
    }

    #[ah_test_utils::logged_test]
    fn test_bearer_auth_headers() {
        let auth = AuthMethod::bearer("jwt-token");
        let mut headers = HeaderMap::new();
        auth.apply_to_headers(&mut headers).unwrap();

        assert_eq!(headers.get("authorization").unwrap(), "Bearer jwt-token");
    }

    #[ah_test_utils::logged_test]
    fn test_auth_config_with_tenant() {
        let config = AuthConfig::with_api_key("token").with_tenant_id("acme");
        let headers = config.headers().unwrap();

        assert_eq!(headers.get("authorization").unwrap(), "ApiKey token");
        assert_eq!(headers.get("x-tenant-id").unwrap(), "acme");
    }
}
