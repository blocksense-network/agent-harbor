# Mock REST API Server

Express.js mock REST API server for the Agent Harbor WebUI development and testing.

## Overview

This mock server implements the [REST-Service.md](../../specs/Public/REST-Service.md) specification to provide a complete API for developing and testing the agent-harbor WebUI and TUI applications. It serves as a development and testing backend that mimics the behavior of the production REST service.

## Quick Start

### Prerequisites

- Node.js >= 22
- npm or yarn

### Installation

#### Install Mock Server Only

```bash
# From the project root
just mock-server-install
```

#### Install All WebUI Dependencies (includes mock server)

```bash
# From the project root (includes shared, app, mock-server, and e2e-tests)
just webui-install
```

### Running the Server

```bash
# From the project root
just webui-mock-server
```

The server will start on `http://localhost:3001` by default in **development mode with hot reloading**.

## API Endpoints

The mock server implements all endpoints defined in [REST-Service.md](../../specs/Public/REST-Service.md):

### Core Endpoints

- `POST /api/v1/tasks` - Create tasks/sessions
- `GET /api/v1/sessions` - List sessions
- `GET /api/v1/sessions/{id}` - Get session details
- `POST /api/v1/sessions/{id}/stop` - Stop session
- `DELETE /api/v1/sessions/{id}` - Cancel session
- `POST /api/v1/sessions/{id}/pause` - Pause session
- `POST /api/v1/sessions/{id}/resume` - Resume session
- `GET /api/v1/sessions/{id}/logs` - Get session logs
- `GET /api/v1/sessions/{id}/events` - SSE stream for real-time updates

### Discovery Endpoints

- `GET /api/v1/agents` - List supported agents
- `GET /api/v1/runtimes` - List available runtimes
- `GET /api/v1/executors` - List execution hosts

### Helper Endpoints

- `GET /api/v1/projects` - List known projects
- `GET /api/v1/repos` - List indexed repositories
- `GET /api/v1/workspaces` - List provisioned workspaces

## Data Flow

```
WebUI/TUI App → Mock Server → Mock Responses
                    ↓
            Integration Tests
```

## Development Workflow

### For WebUI Development

```bash
# Terminal 1: Start mock server
just webui-mock-server

# Terminal 2: Start WebUI
just webui-dev

# Terminal 3: Run tests
just webui-test
```

### For TUI Development

```bash
# Terminal 1: Start mock server
just webui-mock-server

# Terminal 2: Run TUI development
cargo run --bin ah-tui -- --remote-server http://localhost:3001
```

## Technology Stack

- **Runtime**: Node.js 22+
- **Framework**: Express.js
- **Language**: TypeScript
- **Linting**: ESLint
- **Formatting**: Prettier
- **Development**: tsx (for hot reloading)

## Available Scripts

```bash
npm run build        # Build for production
npm run dev          # Development with hot reload (use 'just webui-mock-server' instead)
npm run start        # Start production server
npm run lint         # Run ESLint
npm run lint:fix     # Fix ESLint issues
npm run format       # Format code with Prettier
npm run format:check # Check formatting
npm run type-check   # TypeScript type checking
```

## Mock Data

The server includes comprehensive mock data for:

- Multiple agent types (claude-code, openhands, etc.)
- Various runtime configurations
- Sample sessions with different states
- Realistic repository and project data
- Error scenarios for testing

## Testing Integration

The mock server is designed to work seamlessly with:

- **WebUI E2E Tests**: Playwright tests that drive both the UI and mock server state
- **TUI Integration Tests**: Terminal automation tests against the mock API
- **API Contract Tests**: Verification that the mock server matches REST-Service.md specs

## Configuration

The server runs on port 3001 by default. You can modify the configuration in `src/index.ts` if needed for specific testing scenarios.

## Contributing

When adding new endpoints or modifying existing ones:

1. Update the corresponding route file in `src/routes/`
2. Ensure the implementation matches [REST-Service.md](../../specs/Public/REST-Service.md)
3. Add appropriate mock data and error scenarios
4. Update this README if adding new functionality
5. Test against both WebUI and TUI applications
