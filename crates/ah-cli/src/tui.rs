//! TUI command handling for the CLI

use ah_rest_client::AuthConfig;
use anyhow::Result;
use clap::Args;
use std::process;

/// Arguments for the TUI command
#[derive(Args)]
pub struct TuiArgs {
    /// Remote server URL for REST API connectivity
    #[arg(long, help = "URL of the remote agent-harbor REST service")]
    remote_server: Option<String>,

    /// API key for authentication with remote server
    #[arg(long, help = "API key for authenticating with the remote server")]
    api_key: Option<String>,

    /// Bearer token for authentication with remote server
    #[arg(
        long,
        help = "JWT bearer token for authenticating with the remote server"
    )]
    bearer_token: Option<String>,
}

impl TuiArgs {
    /// Run the TUI command
    pub async fn run(self) -> Result<()> {
        // Validate arguments
        if self.api_key.is_some() && self.bearer_token.is_some() {
            anyhow::bail!("Cannot specify both --api-key and --bearer-token");
        }

        if (self.api_key.is_some() || self.bearer_token.is_some()) && self.remote_server.is_none() {
            anyhow::bail!("--remote-server is required when using authentication");
        }

        // Create authentication config
        let auth = if let Some(api_key) = self.api_key {
            AuthConfig::with_api_key(api_key)
        } else if let Some(bearer_token) = self.bearer_token {
            AuthConfig::with_bearer(bearer_token)
        } else {
            AuthConfig::default()
        };

        // Create REST client if remote server is specified
        let rest_client = if let Some(server_url) = self.remote_server {
            Some(ah_rest_client::RestClient::from_url(&server_url, auth)?)
        } else {
            None
        };

        // Launch the TUI application
        // Note: In a real implementation, this would call ah_tui::App::run()
        // For now, we'll just show a placeholder message
        println!("Launching agent-harbor TUI...");

        if rest_client.is_some() {
            println!("Connected to remote server");
        } else {
            println!("Running in local mode (no remote server configured)");
        }

        // TODO: Actually launch the TUI application
        // This would be: ah_tui::App::new(rest_client)?.run().await?;

        println!("TUI placeholder - press Ctrl+C to exit");

        // For now, just wait for interrupt
        tokio::signal::ctrl_c().await?;
        println!("TUI exited");

        Ok(())
    }
}
