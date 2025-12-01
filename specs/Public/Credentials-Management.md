# Credentials Management

## Summary

The Credentials Management system enables users to maintain multiple accounts for AI agents (Codex, Claude, Cursor, etc.) and seamlessly select appropriate credentials when launching agent sessions. Credentials are securely stored in Agent Harbor's data directory with user-defined labels for easy identification and management.

The system provides:

- **Multi-account support** for each agent type
- **Secure credential storage** with encryption at rest
- **Account selection** during task creation
- **Health monitoring** across all accounts
- **Interactive credential acquisition** via specialized CLI commands

## Architecture

### Storage Structure

Credentials are stored in Agent Harbor's standard configuration directory under a dedicated `credentials/` subdirectory (see [Configuration.md](./Configuration.md) for exact paths):

```
{config-dir}/credentials/
├── accounts.toml          # Account metadata and labels
├── keys/                  # Encrypted credential storage
│   ├── codex-12345.enc
│   ├── claude-work.enc
│   └── cursor-personal.enc
└── temp/                  # Temporary directories for credential acquisition
```

Where `{config-dir}` follows Agent Harbor's user configuration directory in the[configuration hierarchy](./Configuration.md) (typically `~/.config/agent-harbor/` on Linux, `$HOME/Library/Application Support/agent-harbor/` on macOS, or `%APPDATA%/agent-harbor/` on Windows).

### Account Metadata

The `accounts.toml` file maintains account registry with user-friendly names, automatically derived aliases, and per-account encryption settings:

```toml
[[accounts]]
name = "codex-work"
agent = "codex"
aliases = ["john.doe@company.com", "work-codex"]
encrypted = false  # Default: false, can be set to true for sensitive accounts
created = "2025-01-15T10:30:00Z"
last_used = "2025-01-20T14:22:00Z"
status = "active"

[[accounts]]
name = "claude-personal"
agent = "claude"
aliases = ["john.doe@gmail.com", "personal-claude"]
encrypted = false
created = "2025-01-10T09:15:00Z"
last_used = "2025-01-19T16:45:00Z"
status = "active"

[[accounts]]
name = "cursor-sensitive"
agent = "cursor"
aliases = ["john.doe@company.com", "sensitive-cursor"]
encrypted = true   # This account requires passphrase for access
created = "2025-01-12T11:20:00Z"
last_used = "2025-01-18T13:10:00Z"
status = "active"
```

### Credential Encryption (Optional)

Credentials can be optionally encrypted at rest using AES-256-GCM:

- **Default behavior**: Credentials stored in plaintext for immediate access
- **Optional encryption**: Users can enable encryption with a passphrase
- **Master key derivation**: PBKDF2 from user-provided passphrase when encryption enabled
- **Interactive unlock**: Encrypted credentials require passphrase entry on first use per session
- **File format**: Plain JSON by default, encrypted JSON with authentication tags when enabled
- **Key rotation**: Support for credential re-encryption with new keys
- **Backup compatibility**: Encrypted files can be safely backed up and restored

## CLI Commands

### `ah credentials add <agent> [label]`

Interactively acquires and stores credentials for a specific agent:

```bash
# Add a new Codex account
ah credentials add codex "Work Account"

# Add Claude account with auto-generated label
ah credentials add claude

# Add Cursor account for team usage
ah credentials add cursor "Team Cursor"
```

**Process Flow:**

1. **Validation**: Check if agent type is supported and credentials can be acquired
2. **Temporary Environment**: Create isolated `$HOME` directory for credential acquisition
3. **Agent Launch**: Run agent software in interactive mode, expecting user to log in during execution
4. **User Login**: User completes authentication flow within the running agent software
5. **Agent Exit**: Agent software exits after successful login and credential storage
6. **Credential Extraction**: Extract authentication tokens from the temporary environment
7. **Storage**: Store credentials with user-provided name and auto-generated aliases
8. **Verification**: Test credentials by checking account status/limits

**Supported Agents:**

- **Codex**: Launch Codex CLI, user logs in interactively, extract access tokens from `~/.codex/auth.json` after exit
- **Claude**: Launch Claude CLI, user logs in interactively, extract API keys and session tokens after exit
- **Cursor**: Launch Cursor IDE, user logs in interactively, extract session tokens from SQLite database after exit

### `ah credentials list`

Display all stored accounts with current status:

```bash
ah credentials list

Agent Harbor Credentials
═════════════════════════

Codex Accounts:
├── work (john.doe@company.com) - Active
└── personal (john.doe@gmail.com) - Active

Claude Accounts:
├── pro (claude-pro@anthropic.com) - Active
└── team (team@company.com) - Active

Cursor Accounts:
├── personal (john.doe@gmail.com) - Active
└── work (john.doe@company.com) - Expired

Total: 6 accounts
```

**Display Features:**

- **Color coding**: Green for active, yellow for expired, red for invalid
- **Grouping**: Accounts grouped by agent type
- **Status indicators**: Last used timestamp and current validity
- **Compact view**: `--compact` flag for dense output

### `ah credentials remove <account-name>`

Remove stored credentials:

```bash
ah credentials remove codex-work
Removed credentials for account: codex-work
```

**Safety Features:**

- **Confirmation prompt**: Require explicit confirmation for removal
- **Active session check**: Warn if account is currently in use
- **Backup suggestion**: Recommend backing up before removal

### `ah credentials verify <account-name>`

Test credential validity and fetch current limits:

```bash
ah credentials verify codex-work

Verifying credentials for: codex-work (Codex)
✓ Authentication successful
✓ Rate limits retrieved

Plan: team
Primary: 0% used (resets in 2h 15m)
Secondary: 100% used (resets in 16h 30m)
Credits: 500.00 remaining
```

### `ah credentials reauth <account-name>`

Re-acquire credentials for an existing account:

```bash
ah credentials reauth cursor-work
# Launches agent software, user logs in again, updates stored credentials
```

### `ah credentials encrypt <account-name>`

Encrypt a specific account's credentials:

```bash
# Encrypt a sensitive account
ah credentials encrypt cursor-sensitive
# Prompts for passphrase and encrypts the account's credentials

# Encrypt with specific cipher
ah credentials encrypt --cipher aes-256-gcm codex-work
```

### `ah credentials decrypt <account-name>`

Decrypt a specific account's credentials:

```bash
# Decrypt an account (removes encryption)
ah credentials decrypt cursor-sensitive
# Prompts for passphrase and decrypts the account's credentials
```

### `ah credentials encrypt-status [account-name]`

Check encryption status for accounts:

```bash
# Check all accounts
ah credentials encrypt-status

# Check specific account
ah credentials encrypt-status cursor-sensitive
# Shows: "cursor-sensitive: encrypted (AES-256-GCM)"
```

## Agent Session Integration

### Account Selection in `ah agent start`

The `ah agent start` command automatically selects appropriate credentials:

```bash
# Use specific account (implies --agent codex)
ah agent start --account codex-work task-123

# Use default account for agent type
ah agent start task-456  # Uses first active Codex account

# List available accounts for selection
ah agent start --account ? task-789
```

**Selection Logic:**

1. **Explicit account**: Use `--account` flag if provided (implies corresponding `--agent`)
2. **Task metadata**: Check if task specifies required account
3. **Agent default**: Use first active account for the agent type
4. **Interactive selection**: Prompt user if multiple accounts exist and none specified

**Note**: Specifying `--account` automatically infers the corresponding `--agent` type from the account metadata, making `--agent` optional when `--account` is provided.

### Account Resolution

```rust
// Pseudocode for account resolution
fn resolve_account(requested_agent: Option<AgentType>, requested_account: Option<String>) -> Result<Account> {
    // If account is specified, infer agent type from account metadata
    let agent_type = match (requested_agent, &requested_account) {
        (Some(agent), _) => agent,
        (None, Some(account_name)) => get_agent_type_from_account(account_name)?,
        (None, None) => return Err(Error::AgentRequired),
    };

    let accounts = get_accounts_for_agent(agent_type)?;

    match requested_account {
        Some(name) => accounts.get(name).ok_or(Error::AccountNotFound),
        None => {
            // Use most recently used active account
            accounts.iter()
                .filter(|a| a.status == Active)
                .max_by_key(|a| a.last_used)
                .ok_or(Error::NoActiveAccount)
        }
    }
}
```

## TUI Integration

### Account Selection in Launch Options

The TUI's advanced launch options modal includes account selection:

```
┌─ Advanced Launch Options ──────────────────────────┐
│ ┌─ Sandbox & Environment ─┐ ┌─ Launch Actions ─┐    │
│ │ Sandbox Profile: local   │ │ [t] New Tab      │    │
│ │ Working Copy: auto       │ │ [s] Split View   │    │
│ │ FS Snapshots: auto       │ │ [h] Horiz Split  │    │
│ │ Account: ▾ codex-work    │ │ [v] Vert Split   │    │
│ │ Timeout: 30m             │ └─────────────────┘    │
│ │ Output Format: text      │                         │
│ └─────────────────────────┘                         │
└─────────────────────────────────────────────────────┘
```

**Account Selector Features:**

- **Dropdown interface**: Shows available accounts for each selected agent (one dropdown per agent)
- **Status indicators**: Color-coded account status (active/expired/invalid)
- **Quick selection**: Keyboard shortcuts for frequently used accounts
- **Account details**: Hover/selection shows account metadata (email, creation date)

## Health Monitoring Integration

### Enhanced `ah health` Command

The `ah health` command now includes credential status across all accounts:

```bash
ah health

Agent Harbor Health Report
═══════════════════════════

System Status: ✓ Healthy
Database: ✓ Connected
Credentials: 6 accounts (5 active, 1 expired)

Codex Accounts:
├── work (john.doe@company.com)
│   ├── Status: ✓ Active
│   ├── Plan: team
│   ├── Primary: 0% used (resets 2h 15m)
│   └── Secondary: 100% used (resets 16h 30m)
└── personal (john.doe@gmail.com)
    ├── Status: ✓ Active
    ├── Plan: free
    ├── Primary: 45% used (resets 4h 30m)
    └── Secondary: 0% used (resets 1d 2h)

Claude Accounts:
├── pro (claude-pro@anthropic.com)
│   ├── Status: ✓ Active
│   └── Usage: 234 tokens (2.1% of limit)
└── team (team@company.com)
    ├── Status: ✓ Active
    └── Usage: 1,247 tokens (12.3% of limit)

Cursor Accounts:
├── personal (john.doe@gmail.com)
│   ├── Status: ✓ Active
│   └── Plan: pro
└── work (john.doe@company.com)
    ├── Status: ⚠️ Expired
    └── Last valid: 2025-01-18 13:10:00

Issues: 1 expired account
Recommendations:
- Re-authenticate cursor-work account
```

**Health Features:**

- **Account status overview**: Summary of total accounts and their states
- **Per-account details**: Current usage, limits, and expiration status
- **Color coding**: Green for healthy, yellow for warnings, red for errors
- **Usage metrics**: Current consumption percentages and reset times
- **Actionable recommendations**: Specific steps to resolve issues

## Security Considerations

### Credential Protection

- **Default storage**: Credentials stored in plaintext for immediate access without unlock
- **Per-account encryption**: Individual accounts can be encrypted while others remain accessible
- **Selective security**: Encrypt only sensitive accounts (e.g., production, company accounts)
- **Master key management**: PBKDF2-derived keys with configurable rounds per encrypted account
- **Interactive unlock**: Encrypted accounts require passphrase entry on first use per session
- **Key rotation**: Support for credential re-encryption with new keys for individual accounts
- **Memory safety**: Credentials decrypted only in memory, zeroed after use. When credentials are written to disk to provision agent sessions, we make sure to properly clean them up at the end of the session.

### Access Control

- **File permissions**: Credential files readable only by owner
- **Audit logging**: Track credential access and usage
- **Expiration handling**: Automatic cleanup of expired sessions

### Credential Acquisition Security

- **Isolated execution**: Agent software runs in temporary directories
- **Network monitoring**: Detect and prevent credential exfiltration
- **Session cleanup**: Ensure no persistent credential storage in temp directories
- **Validation**: Verify acquired credentials before storage

## Configuration

### Credential Storage Settings

Credential storage configuration follows Agent Harbor's standard [configuration hierarchy](./Configuration.md):

```toml
[credentials]
# Storage location (relative to config directory)
storage-path = "credentials"

# Global encryption defaults (optional)
encryption-enabled = false  # Default encryption for new accounts
encryption-cipher = "aes-256-gcm"  # Default cipher for encryption
pbkdf2-iterations = 100000

# Auto-verification settings
auto-verify-interval = "24h"  # Check credential validity periodically
auto-verify-on-start = true  # Verify credentials when Agent Harbor starts

# Default account selection
default-accounts = [
  { agent = "codex", account = "work" },
  { agent = "claude", account = "personal" },
]
```

**Note**: Individual accounts can override the global `encryption-enabled` setting with their own `encrypted` field in `accounts.toml`.

### TUI Settings

```toml
[tui.credentials]
# Default account selection behavior
default-credentials = "last-used" # other options are `always-ask` and `use-preferred-account`

# Account selector style
account_selector_style = "dropdown"  # dropdown, list, compact
```

When `use-preferred-account` is selected, check whether the user has multiple credentials for the selected agent type. If there are multiple credentials, require that only one of them is marked with the property `preferred = true`.

## Implementation Notes

### Credential Storage Format

Each credential file contains encrypted JSON with agent-specific data:

```json
{
  "version": "1.0",
  "agent": "codex",
  "account_name": "codex-work",
  "aliases": ["john.doe@company.com", "work-codex"],
  "created": "2025-01-15T10:30:00Z",
  "credentials": {
    "access_token": "eyJ...",
    "refresh_token": "refresh_123...",
    "expires_at": "2025-02-15T10:30:00Z"
  },
  "metadata": {
    "email": "john.doe@company.com",
    "plan": "team",
    "verified_at": "2025-01-20T14:22:00Z"
  }
}
```

### Account Resolution Algorithm

1. **Explicit specification**: Use `--account` flag or TUI selection (account implies agent type)
2. **Task configuration**: Check task metadata for required account
3. **Default mapping**: Use `default-accounts` configuration
4. **Most recent**: Fall back to most recently used active account
5. **Interactive**: Prompt user for selection if ambiguous
6. **Encryption handling**: For encrypted accounts, prompt for passphrase if not already unlocked in current session

### Migration Path

For existing single-account users:

1. **Automatic migration**: Convert existing credentials to new format
2. **Backward compatibility**: Maintain support for old credential locations
3. **Gradual adoption**: New features work alongside existing setup
4. **Migration prompts**: Guide users through credential organization

## Error Handling

### Common Error Scenarios

- **Invalid credentials**: Clear error messages with re-authentication instructions
- **Expired accounts**: Automatic detection with renewal prompts
- **Network failures**: Retry logic with exponential backoff
- **Storage corruption**: Recovery mechanisms and backup restoration

### User-Friendly Messages

```
Error: Credentials for 'codex-work' have expired
Solution: Run 'ah credentials reauth codex-work' to refresh
```

```
Error: No active accounts found for Claude
Solution: Run 'ah credentials add claude' to add an account
```

## Testing Strategy

### Unit Tests

- **Encryption/decryption**: Verify credential protection
- **Account resolution**: Test selection algorithms
- **Storage operations**: CRUD operations on account metadata

### Integration Tests

- **Credential acquisition**: End-to-end credential capture workflows
- **Agent provisioning**: Verify correct account selection in `ah agent start`
- **TUI integration**: Account selection in launch dialogs

### End-to-End Tests

- **Full workflow**: Create account → Launch task → Verify usage tracking
- **Multi-account scenarios**: Test account switching and selection
- **Health monitoring**: Validate `ah health` output accuracy
