import { Component, createSignal, createResource, Show, For } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { apiClient } from "../../lib/api.js";

interface TaskDetailsPageProps {
  taskId?: string;
}

export const TaskDetailsPage: Component<TaskDetailsPageProps> = (props) => {
  const params = useParams();
  const navigate = useNavigate();
  const taskId = () => props.taskId || params["id"];

  const [activeTab, setActiveTab] = createSignal("overview");

  // Load task details from API
  const [taskData] = createResource(taskId, async (id) => {
    if (!id) return null;
    try {
      const result = await apiClient.getSession(id);
      return result;
    } catch (error) {
      console.error("Failed to load task details:", error);
      return null;
    }
  });

  // Load logs from API
  const [logsData] = createResource(taskId, async (id) => {
    try {
      const result = await apiClient.getSessionLogs(id);
      return result.logs || [];
    } catch (error) {
      console.error("Failed to load logs:", error);
      return [];
    }
  });

  const task = () => taskData();
  const logs = () => logsData();

  const handleBack = () => {
    navigate("/");
  };

  const handleStop = async () => {
    if (!taskId()) return;

    try {
      await apiClient.stopSession(taskId()!);
      // Note: SolidJS resources don't have a refetch method
      // The UI will update automatically when the session state changes
    } catch (error) {
      console.error("Failed to stop task:", error);
    }
  };

  const handlePause = async () => {
    if (!taskId()) return;

    try {
      // For now, just show a placeholder - pause/resume would need API endpoints
      console.log("Pause task:", taskId());
    } catch (error) {
      console.error("Failed to pause task:", error);
    }
  };

  const handleResume = async () => {
    if (!taskId()) return;

    try {
      // For now, just show a placeholder - pause/resume would need API endpoints
      console.log("Resume task:", taskId());
    } catch (error) {
      console.error("Failed to resume task:", error);
    }
  };

  const handleLaunchIDE = async () => {
    if (!taskId()) return;

    try {
      // For now, just show a placeholder - IDE launch would need API endpoint
      console.log("Launch IDE for task:", taskId());
    } catch (error) {
      console.error("Failed to launch IDE:", error);
    }
  };

  return (
    <Show
      when={task()}
      fallback={
        <div class="flex min-h-screen items-center justify-center bg-gray-50">
          <div class="text-center">
            <h2 class="mb-2 text-xl font-semibold text-gray-900">
              Task not found
            </h2>
            <p class="text-gray-600">The requested task could not be loaded.</p>
          </div>
        </div>
      }
    >
      <div class="min-h-screen bg-gray-50">
        {/* Header */}
        <header class="border-b border-gray-200 bg-white px-6 py-4">
          <div class="flex items-center justify-between">
            <div class="flex items-center space-x-4">
              <button
                onClick={handleBack}
                class={`
                p-2 text-gray-600
                hover:text-gray-900
              `}
              >
                <svg
                  class="h-5 w-5"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M15 19l-7-7 7-7"
                  />
                </svg>
              </button>
              <div>
                <h1 class="text-xl font-semibold text-gray-900">
                  Task Details
                </h1>
                <p class="text-sm text-gray-600">ID: {taskId()}</p>
              </div>
            </div>

            <div class="flex items-center space-x-3">
              <button
                onClick={handleLaunchIDE}
                class={`
                rounded-md border border-transparent bg-green-600 px-4 py-2
                text-sm font-medium text-white
                hover:bg-green-700
                focus:ring-2 focus:ring-green-500 focus:ring-offset-2
                focus:outline-none
              `}
              >
                Launch IDE
              </button>

              <Show when={task()?.status === "running"}>
                <button
                  onClick={handlePause}
                  class={`
                  rounded-md border border-transparent bg-yellow-600 px-4 py-2
                  text-sm font-medium text-white
                  hover:bg-yellow-700
                  focus:ring-2 focus:ring-yellow-500 focus:ring-offset-2
                  focus:outline-none
                `}
                >
                  Pause
                </button>
              </Show>

              <Show when={task()?.status === "paused"}>
                <button
                  onClick={handleResume}
                  class={`
                  rounded-md border border-transparent bg-green-600 px-4 py-2
                  text-sm font-medium text-white
                  hover:bg-green-700
                  focus:ring-2 focus:ring-green-500 focus:ring-offset-2
                  focus:outline-none
                `}
                >
                  Resume
                </button>
              </Show>

              <Show when={task()?.status === "running"}>
                <button
                  onClick={handleStop}
                  class={`
                  rounded-md border border-transparent bg-red-600 px-4 py-2
                  text-sm font-medium text-white
                  hover:bg-red-700
                  focus:ring-2 focus:ring-red-500 focus:ring-offset-2
                  focus:outline-none
                `}
                >
                  Stop
                </button>
              </Show>
            </div>
          </div>
        </header>

        <div class="mx-auto max-w-7xl px-6 py-6">
          <Show when={task()}>
            <div
              class={`
              grid grid-cols-1 gap-6
              lg:grid-cols-3
            `}
            >
              {/* Left Column - Task Info */}
              <div class="lg:col-span-1">
                <div
                  class={`
                  rounded-lg border border-gray-200 bg-white p-6 shadow-sm
                `}
                >
                  <h2 class="mb-4 text-lg font-semibold text-gray-900">
                    Task Information
                  </h2>

                  <div class="space-y-4">
                    <div>
                      <label class="mb-1 block text-sm font-medium text-gray-700">
                        Status
                      </label>
                      <div class="flex items-center space-x-2">
                        <span
                          class={`
                          inline-flex items-center rounded-full px-2.5 py-0.5
                          text-xs font-medium
                          ${
                            task()!.status === "completed"
                              ? "bg-green-100 text-green-800"
                              : task()!.status === "running"
                                ? "bg-blue-100 text-blue-800"
                                : task()!.status === "failed"
                                  ? "bg-red-100 text-red-800"
                                  : "bg-gray-100 text-gray-800"
                          }
                        `}
                        >
                          {task()!.status}
                        </span>
                      </div>
                    </div>

                    <div>
                      <label class="mb-1 block text-sm font-medium text-gray-700">
                        Repository
                      </label>
                      <p class="text-sm text-gray-900">
                        {task()!.repo.url || "Unknown"}
                      </p>
                    </div>

                    <div>
                      <label class="mb-1 block text-sm font-medium text-gray-700">
                        Branch
                      </label>
                      <p class="text-sm text-gray-900">
                        {task()!.repo.branch || "main"}
                      </p>
                    </div>

                    <div>
                      <label class="mb-1 block text-sm font-medium text-gray-700">
                        Agent
                      </label>
                      <p class="text-sm text-gray-900">
                        {task()!.agent.type} {task()!.agent.version}
                      </p>
                    </div>

                    <div>
                      <label class="mb-1 block text-sm font-medium text-gray-700">
                        Created
                      </label>
                      <p class="text-sm text-gray-900">
                        {new Date(task()!.createdAt).toLocaleString()}
                      </p>
                    </div>
                  </div>
                </div>
              </div>

              {/* Right Column - Tabs */}
              <div class="lg:col-span-2">
                <div class="rounded-lg border border-gray-200 bg-white shadow-sm">
                  {/* Tab Navigation */}
                  <div class="border-b border-gray-200">
                    <nav class="flex">
                      <For
                        each={[
                          { id: "overview", label: "Overview" },
                          { id: "logs", label: "Live Log" },
                          { id: "events", label: "Events" },
                          { id: "report", label: "Report" },
                          { id: "workspace", label: "Workspace" },
                        ]}
                      >
                        {(tab) => (
                          <button
                            onClick={() => setActiveTab(tab.id)}
                            class={`
                            border-b-2 px-4 py-3 text-sm font-medium
                            transition-colors
                            ${
                              activeTab() === tab.id
                                ? "border-blue-500 text-blue-600"
                                : `
                                  border-transparent text-gray-500
                                  hover:border-gray-300 hover:text-gray-700
                                `
                            }
                          `}
                          >
                            {tab.label}
                          </button>
                        )}
                      </For>
                    </nav>
                  </div>

                  {/* Tab Content */}
                  <div class="p-6">
                    <Show when={activeTab() === "overview"}>
                      <div>
                        <h3 class="mb-4 text-lg font-medium text-gray-900">
                          Task Overview
                        </h3>
                        <div class="prose prose-sm max-w-none">
                          <p class="whitespace-pre-wrap text-gray-700">
                            {task()!.prompt}
                          </p>
                        </div>
                      </div>
                    </Show>

                    <Show when={activeTab() === "logs"}>
                      <div>
                        <h3 class="mb-4 text-lg font-medium text-gray-900">
                          Live Log
                        </h3>
                        <div
                          class={`
                          max-h-96 overflow-y-auto rounded-lg bg-gray-900 p-4
                          font-mono text-sm text-green-400
                        `}
                        >
                          <Show when={(logs() || []).length > 0}>
                            <For each={logs()}>
                              {(log, _index) => {
                                const logMessage =
                                  typeof log === "string"
                                    ? log
                                    : log.message || "";
                                const isError =
                                  logMessage.toLowerCase().includes("error") ||
                                  logMessage.toLowerCase().includes("failed");
                                const isWarning =
                                  logMessage.toLowerCase().includes("warn") ||
                                  logMessage.toLowerCase().includes("warning");

                                return (
                                  <div class="mb-1">
                                    <span class="text-gray-400">
                                      [
                                      {task()
                                        ? new Date(
                                            task()!.createdAt,
                                          ).toLocaleTimeString()
                                        : ""}
                                      ]
                                    </span>
                                    <span
                                      class={`
                                      ml-2
                                      ${
                                        isError
                                          ? "text-red-400"
                                          : isWarning
                                            ? "text-yellow-400"
                                            : "text-green-400"
                                      }
                                    `}
                                    >
                                      {logMessage}
                                    </span>
                                  </div>
                                );
                              }}
                            </For>
                          </Show>
                          <Show when={(logs() || []).length === 0}>
                            <div class="text-gray-500">
                              No logs available yet...
                            </div>
                          </Show>
                        </div>
                      </div>
                    </Show>

                    <Show when={activeTab() === "events"}>
                      <div>
                        <h3 class="mb-4 text-lg font-medium text-gray-900">
                          Events
                        </h3>
                        <div class="text-sm text-gray-500">
                          Events timeline will be displayed here...
                        </div>
                      </div>
                    </Show>

                    <Show when={activeTab() === "report"}>
                      <div>
                        <h3 class="mb-4 text-lg font-medium text-gray-900">
                          Report
                        </h3>
                        <div class="text-sm text-gray-500">
                          Task report and diff will be displayed here...
                        </div>
                      </div>
                    </Show>

                    <Show when={activeTab() === "workspace"}>
                      <div>
                        <h3 class="mb-4 text-lg font-medium text-gray-900">
                          Workspace
                        </h3>
                        <div class="text-sm text-gray-500">
                          Workspace information and IDE launch helpers will be
                          displayed here...
                        </div>
                      </div>
                    </Show>
                  </div>
                </div>
              </div>
            </div>
          </Show>

          <Show when={!task()}>
            <div class="py-12 text-center">
              <svg
                class="mx-auto h-12 w-12 text-gray-400"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M9 5H7a2 2 0 00-2 2v10a2 2 0 002 2h8a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
                />
              </svg>
              <h3 class="mt-2 text-sm font-medium text-gray-900">
                Task not found
              </h3>
              <p class="mt-1 text-sm text-gray-500">
                The requested task could not be found.
              </p>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};
