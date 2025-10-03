// API client for the Agent Harbor REST service

export interface Repository {
  mode: "git" | "upload" | "none";
  url?: string;
  branch?: string;
  commit?: string;
}

export interface Runtime {
  type: "devcontainer" | "local" | "disabled";
  devcontainerPath?: string;
  resources?: {
    cpu?: number;
    memoryMiB?: number;
  };
}

export interface Agent {
  type: string;
  version: string;
  settings?: Record<string, unknown>;
}

export interface Delivery {
  mode: "pr" | "branch" | "patch";
  targetBranch?: string;
}

export interface CreateTaskRequest {
  tenantId?: string;
  projectId?: string;
  prompt: string;
  repo: Repository;
  runtime: Runtime;
  agent: Agent;
  delivery?: Delivery;
  labels?: Record<string, string>;
  webhooks?: Array<{
    event: string;
    url: string;
  }>;
}

export interface CreateTaskResponse {
  id: string;
  status: string;
  links: {
    self: string;
    events: string;
    logs: string;
  };
}

export interface Session {
  id: string;
  tenantId?: string;
  projectId?: string;
  status:
    | "queued"
    | "provisioning"
    | "running"
    | "pausing"
    | "paused"
    | "resuming"
    | "stopping"
    | "stopped"
    | "completed"
    | "failed"
    | "cancelled";
  createdAt: string;
  prompt: string;
  repo: Repository;
  runtime: Runtime;
  agent: Agent;
  delivery?: Delivery;
  labels?: Record<string, string>;
  links: {
    self: string;
    events: string;
    logs: string;
  };
  // Last 3 events for SSR pre-population (active sessions only)
  recent_events?: (SessionEvent | Record<string, unknown>)[];
}

export interface SessionsListResponse {
  items: Session[];
  pagination: {
    page: number;
    perPage: number;
    total: number;
    totalPages: number;
  };
}

export interface AgentType {
  type: string;
  versions: string[];
  settingsSchemaRef?: string;
}

export interface RuntimeType {
  type: string;
  images?: string[];
  paths?: string[];
  sandboxProfiles?: string[];
}

export interface LogEntry {
  level: string;
  message: string;
  ts: string;
}

export interface LogsResponse {
  logs: LogEntry[];
}

export interface RepositoryItem {
  id: string;
  name: string;
  branch: string;
  lastCommit: string;
  url?: string;
}

// Server contract for creating drafts
export interface DraftCreate {
  prompt: string;
  repo: {
    mode: "git" | "upload" | "none";
    url?: string;
    branch?: string;
  };
  agents: Array<{
    type: string;
    version: string;
    instances: number;
  }>;
  runtime: {
    type: "devcontainer" | "local" | "disabled";
  };
  delivery: {
    mode: "pr" | "branch" | "patch";
  };
}

// Full draft task returned by server (includes server-managed fields)
export interface DraftTask extends DraftCreate {
  id: string;
  createdAt: string;
  updatedAt: string;
}

// Updates to draft tasks (excludes server-managed fields)
export type DraftUpdate = Partial<
  Omit<DraftTask, "id" | "createdAt" | "updatedAt">
>;

export interface RepositoriesListResponse {
  items: RepositoryItem[];
  pagination: {
    page: number;
    perPage: number;
    total: number;
    totalPages: number;
  };
}

// Discriminated union for session events
export type SessionEvent =
  | StatusEvent
  | LogEvent
  | ProgressEvent
  | ThinkingEvent
  | ToolExecutionEvent
  | FileEditEvent;

export interface StatusEvent {
  type: "status";
  sessionId: string;
  status: string;
  ts: string;
}

export interface LogEvent {
  type: "log";
  sessionId: string;
  level: string;
  message: string;
  ts: string;
}

export interface ProgressEvent {
  type: "progress";
  sessionId: string;
  progress: number;
  stage: string;
  ts: string;
}

export interface ThinkingEvent {
  type: "thinking";
  sessionId: string;
  thought: string;
  ts: string;
}

export interface ToolExecutionEvent {
  type: "tool_execution";
  sessionId: string;
  tool_name: string;
  tool_args: Record<string, unknown>;
  tool_output?: string;
  tool_status?: string;
  last_line?: string;
  ts: string;
}

export interface FileEditEvent {
  type: "file_edit";
  sessionId: string;
  file_path: string;
  lines_added?: number;
  lines_removed?: number;
  diff_preview?: string;
  ts: string;
}

export type SessionEventHandler = (event: SessionEvent) => void;

export interface ApiError {
  type: string;
  title: string;
  status: number;
  detail: string;
  errors?: Record<string, string[]>;
}

// API base URL - proxy-based architecture
// The SSR server proxies all /api/v1/* requests to the access point daemon
// Client uses relative URLs; SSR uses absolute localhost URLs
const API_BASE =
  typeof window !== "undefined"
    ? "/api/v1" // Browser: use proxy
    : "http://localhost:3002/api/v1"; // SSR: call SSR server's proxy

class ApiClient {
  private async request<T>(
    endpoint: string,
    options: RequestInit = {},
  ): Promise<T> {
    const url = `${API_BASE}${endpoint}`;

    const response = await fetch(url, {
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
      ...options,
    });

    if (!response.ok) {
      const errorData: ApiError = await response.json().catch(() => ({
        type: "unknown",
        title: "Network Error",
        status: response.status,
        detail: response.statusText,
      }));
      throw errorData;
    }

    return response.json();
  }

  // Task operations
  async createTask(data: CreateTaskRequest): Promise<CreateTaskResponse> {
    return this.request("/tasks", {
      method: "POST",
      body: JSON.stringify(data),
    });
  }

  // Session operations
  async listSessions(params?: {
    status?: string;
    projectId?: string;
    page?: number;
    perPage?: number;
  }): Promise<SessionsListResponse> {
    const searchParams = new URLSearchParams();
    if (params?.status) searchParams.set("status", params.status);
    if (params?.projectId) searchParams.set("projectId", params.projectId);
    if (params?.page) searchParams.set("page", params.page.toString());
    if (params?.perPage) searchParams.set("perPage", params.perPage.toString());

    const query = searchParams.toString();
    return this.request(`/sessions${query ? `?${query}` : ""}`);
  }

  async getSession(id: string): Promise<Session> {
    return this.request(`/sessions/${id}`);
  }

  async stopSession(id: string): Promise<{ status: string }> {
    return this.request(`/sessions/${id}/stop`, {
      method: "POST",
    });
  }

  async cancelSession(id: string): Promise<void> {
    await this.request(`/sessions/${id}`, {
      method: "DELETE",
    });
  }

  async pauseSession(id: string): Promise<{ status: string }> {
    return this.request(`/sessions/${id}/pause`, {
      method: "POST",
    });
  }

  async resumeSession(id: string): Promise<{ status: string }> {
    return this.request(`/sessions/${id}/resume`, {
      method: "POST",
    });
  }

  async getSessionLogs(id: string, tail?: number): Promise<LogsResponse> {
    const params = tail ? `?tail=${tail}` : "";
    return this.request(`/sessions/${id}/logs${params}`);
  }

  // SSE event streaming
  subscribeToSessionEvents(
    id: string,
    onEvent: SessionEventHandler,
  ): EventSource {
    const eventSource = new EventSource(`${API_BASE}/sessions/${id}/events`);

    // Handle all event types sent by the server
    const handleEvent = (event: MessageEvent<string>) => {
      try {
        const parsedData = JSON.parse(event.data);
        // Add type field based on SSE event type for discriminated union
        const data: SessionEvent = {
          ...parsedData,
          type: event.type as SessionEvent["type"],
        };
        onEvent(data);
      } catch (error) {
        console.error("Failed to parse SSE event:", error);
      }
    };

    // Listen for specific event types
    eventSource.addEventListener("status", handleEvent);
    eventSource.addEventListener("thinking", handleEvent);
    eventSource.addEventListener("tool_execution", handleEvent);
    eventSource.addEventListener("file_edit", handleEvent);
    eventSource.addEventListener("progress", handleEvent);
    eventSource.addEventListener("log", handleEvent);

    // Fallback for generic messages
    eventSource.onmessage = handleEvent;

    eventSource.onerror = (error) => {
      console.error("SSE connection error:", error);
    };

    return eventSource;
  }

  // Metadata operations
  async listAgents(): Promise<{ items: AgentType[] }> {
    return this.request("/agents");
  }

  async listRuntimes(): Promise<{ items: RuntimeType[] }> {
    return this.request("/runtimes");
  }

  async listRepositories(params?: {
    page?: number;
    perPage?: number;
  }): Promise<RepositoriesListResponse> {
    const searchParams = new URLSearchParams();
    if (params?.page) searchParams.set("page", params.page.toString());
    if (params?.perPage) searchParams.set("perPage", params.perPage.toString());

    const query = searchParams.toString();
    return this.request(`/repositories${query ? `?${query}` : ""}`);
  }

  async getRepository(id: string): Promise<RepositoryItem> {
    return this.request(`/repositories/${id}`);
  }

  // Draft operations
  async listDrafts(): Promise<{ items: DraftTask[] }> {
    return this.request("/drafts");
  }

  async createDraft(draft: DraftCreate): Promise<DraftTask> {
    return this.request("/drafts", {
      method: "POST",
      body: JSON.stringify(draft),
    });
  }

  async updateDraft(id: string, updates: DraftUpdate): Promise<DraftTask> {
    return this.request(`/drafts/${id}`, {
      method: "PUT",
      body: JSON.stringify(updates),
    });
  }

  async deleteDraft(id: string): Promise<void> {
    await this.request(`/drafts/${id}`, {
      method: "DELETE",
    });
  }
}

export const apiClient = new ApiClient();
