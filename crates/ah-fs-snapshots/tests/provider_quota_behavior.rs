// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared behavior for providers that support quota/space limit testing.
//!
//! This module contains quota testing behaviors ported from the legacy Ruby
//! provider_quota_test_behavior.rb.

/// Quota testing behavior for providers that support space limits.
pub trait ProviderQuotaTestBehavior {
    /// Whether this provider supports quota testing.
    fn supports_quota_testing(&self) -> bool {
        false
    }

    /// Setup quota environment for testing.
    fn setup_quota_environment(&self) -> ah_fs_snapshots_traits::Result<()> {
        // Override in subclasses that support quota testing
        Ok(())
    }

    /// Cleanup quota environment after testing.
    fn cleanup_quota_environment(&self) -> ah_fs_snapshots_traits::Result<()> {
        // Override in subclasses that support quota testing
        Ok(())
    }

    /// Default quota test size in bytes (15MB).
    fn quota_test_size(&self) -> u64 {
        15 * 1024 * 1024
    }

    /// Verify quota behavior after testing.
    fn verify_quota_behavior(
        &self,
        quota_exceeded: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Override in subclasses for provider-specific quota verification
        use std::io::{self, Write};
        if quota_exceeded {
            let _ = writeln!(io::stdout(), "Quota was properly enforced");
        } else {
            let _ = writeln!(
                io::stdout(),
                "Quota was not enforced or test conditions not met"
            );
        }
        Ok(())
    }
}
