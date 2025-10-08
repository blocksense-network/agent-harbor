//! Authentication and authorization

use crate::error::ServerError;
use axum::{
    extract::Request,
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// Authentication configuration
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub api_key: Option<String>,
    pub jwt_secret: Option<String>,
}

impl AuthConfig {
    /// Create auth config from API key
    pub fn with_api_key(api_key: String) -> Self {
        Self {
            api_key: Some(api_key),
            jwt_secret: None,
        }
    }

    /// Create auth config from JWT secret
    pub fn with_jwt_secret(secret: String) -> Self {
        Self {
            api_key: None,
            jwt_secret: Some(secret),
        }
    }

    /// Check if authentication is required
    pub fn requires_auth(&self) -> bool {
        self.api_key.is_some() || self.jwt_secret.is_some()
    }

    /// Validate API key authentication
    pub fn validate_api_key(&self, provided_key: &str) -> Result<(), ServerError> {
        if let Some(expected_key) = &self.api_key {
            if expected_key == provided_key {
                Ok(())
            } else {
                Err(ServerError::Auth("Invalid API key".to_string()))
            }
        } else {
            Err(ServerError::Auth("API key authentication not configured".to_string()))
        }
    }

    /// Validate JWT token
    pub fn validate_jwt(&self, token: &str) -> Result<Claims, ServerError> {
        if let Some(secret) = &self.jwt_secret {
            let decoding_key = DecodingKey::from_secret(secret.as_ref());
            let validation = Validation::default();

            let token_data = decode::<Claims>(token, &decoding_key, &validation)
                .map_err(|_| ServerError::Auth("Invalid JWT token".to_string()))?;

            Ok(token_data.claims)
        } else {
            Err(ServerError::Auth("JWT authentication not configured".to_string()))
        }
    }

    /// Extract authentication headers for HTTP requests
    pub fn headers(&self) -> Result<HeaderMap, ServerError> {
        let mut headers = HeaderMap::new();

        if let Some(api_key) = &self.api_key {
            headers.insert(
                "Authorization",
                format!("ApiKey {}", api_key).parse().unwrap(),
            );
        }

        Ok(headers)
    }
}

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // Subject (user ID)
    pub exp: usize,       // Expiration time
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub roles: Vec<String>,
}

/// Authentication middleware
pub async fn auth_middleware(
    auth_config: AuthConfig,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip authentication for health checks and OpenAPI docs
    let path = req.uri().path();
    if path == "/healthz" || path == "/readyz" || path == "/version"
        || path.starts_with("/docs/") || path == "/openapi.json" {
        return Ok(next.run(req).await);
    }

    // Extract authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let auth_result = match auth_header {
        Some(auth) if auth.starts_with("ApiKey ") => {
            let api_key = auth.trim_start_matches("ApiKey ");
            auth_config.validate_api_key(api_key)
        }
        Some(auth) if auth.starts_with("Bearer ") => {
            let token = auth.trim_start_matches("Bearer ");
            auth_config.validate_jwt(token).map(|_| ())
        }
        _ => {
            if auth_config.requires_auth() {
                Err(ServerError::Auth("Missing or invalid authorization header".to_string()))
            } else {
                Ok(())
            }
        }
    };

    match auth_result {
        Ok(_) => Ok(next.run(req).await),
        Err(err) => {
            let response = err.into_response();
            Ok(response)
        }
    }
}
