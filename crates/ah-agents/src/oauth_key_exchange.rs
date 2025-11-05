// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// OAuth 2.0 Token Exchange implementation following RFC 8693
/// This module handles exchanging OAuth tokens for API keys
use crate::traits::{AgentError, AgentResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// OAuth token exchange request parameters
#[derive(Debug, Serialize)]
struct TokenExchangeRequest {
    grant_type: String,
    client_id: String,
    requested_token: String,
    subject_token: String,
    subject_token_type: String,
}

/// OAuth token exchange response
#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    #[allow(unused)]
    token_type: String,
    #[allow(unused)]
    expires_in: Option<u32>,
}

/// Exchange an OAuth ID token for an OpenAI API key using RFC 8693 Token Exchange
pub async fn exchange_oauth_for_openai_api_key(
    id_token: &str,
    client_id: &str,
) -> AgentResult<String> {
    let client = Client::new();
    let token_url = "https://api.openai.com/oauth/token";

    let request = TokenExchangeRequest {
        grant_type: "urn:ietf:params:oauth:grant-type:token-exchange".to_string(),
        client_id: client_id.to_string(),
        requested_token: "openai-api-key".to_string(),
        subject_token: id_token.to_string(),
        subject_token_type: "urn:ietf:params:oauth:token-type:id_token".to_string(),
    };

    debug!("Performing OAuth token exchange for OpenAI API key");

    let response = client.post(token_url).form(&request).send().await.map_err(|e| {
        AgentError::CredentialCopyFailed(format!("Token exchange request failed: {}", e))
    })?;

    if !response.status().is_success() {
        return Err(AgentError::CredentialCopyFailed(format!(
            "Token exchange failed with status: {}",
            response.status()
        )));
    }

    let response_json: TokenExchangeResponse = response.json().await.map_err(|e| {
        AgentError::CredentialCopyFailed(format!("Failed to parse token exchange response: {}", e))
    })?;

    // Validate that we got an API key (should start with "sk-")
    if response_json.access_token.starts_with("sk-") {
        debug!("Successfully exchanged OAuth token for OpenAI API key");
        Ok(response_json.access_token)
    } else {
        Err(AgentError::CredentialCopyFailed(
            "Token exchange response does not contain a valid API key".to_string(),
        ))
    }
}

/// Exchange an OAuth access token for an Anthropic API key
/// Note: This is a placeholder - Anthropic may have different token exchange requirements
pub async fn exchange_oauth_for_anthropic_api_key(_access_token: &str) -> AgentResult<String> {
    warn!("OAuth token exchange for Anthropic API keys is not yet implemented");
    // For now, return an error to indicate this needs to be implemented
    Err(AgentError::CredentialCopyFailed(
        "OAuth token exchange for Anthropic not implemented".to_string(),
    ))
}
