import { Component, createSignal, createResource, Show, For } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { apiClient } from "../../lib/api.js";

import { Session } from "../../lib/api.js";

interface TaskDetails extends Session {
  logs: string[];
}

interface TaskDetailsPageProps {
  taskId?: string;
}

export const TaskDetailsPage: Component<TaskDetailsPageProps> = (props) => {
  const params = useParams();
  const navigate = useNavigate();
  const taskId = () => props.taskId || params.id;

  const [activeTab, setActiveTab] = createSignal("overview");

  // Load task details from API
  const [taskData] = createResource(taskId, async (id) => {
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
      // Refresh task data
      await taskData.refetch?.();
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
    <div class="min-h-screen bg-gray-50">
      {/* Header */}
      <header class="bg-white border-b border-gray-200 px-6 py-4">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-4">
            <button
              onClick={handleBack}
              class="text-gray-600 hover:text-gray-900 p-2"
            >
              <svg
                class="w-5 h-5"
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
              <h1 class="text-xl font-semibold text-gray-900">Task Details</h1>
              <p class="text-sm text-gray-600">ID: {taskId()}</p>
            </div>
          </div>

          <div class="flex items-center space-x-3">
            <button
              onClick={handleLaunchIDE}
              class="px-4 py-2 text-sm font-medium text-white bg-green-600 border border-transparent rounded-md hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500"
            >
              Launch IDE
            </button>

            <Show when={task()?.status === "running"}>
              <button
                onClick={handlePause}
                class="px-4 py-2 text-sm font-medium text-white bg-yellow-600 border border-transparent rounded-md hover:bg-yellow-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-yellow-500"
              >
                Pause
              </button>
            </Show>

            <Show when={task()?.status === "paused"}>
              <button
                onClick={handleResume}
                class="px-4 py-2 text-sm font-medium text-white bg-green-600 border border-transparent rounded-md hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500"
              >
                Resume
              </button>
            </Show>

            <Show when={task()?.status === "running"}>
              <button
                onClick={handleStop}
                class="px-4 py-2 text-sm font-medium text-white bg-red-600 border border-transparent rounded-md hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-red-500"
              >
                Stop
              </button>
            </Show>
          </div>
        </div>
      </header>

      <div class="max-w-7xl mx-auto px-6 py-6">
        <Show when={task()}>
          <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Left Column - Task Info */}
            <div class="lg:col-span-1">
              <div class="bg-white rounded-lg shadow-sm border border-gray-200 p-6">
                <h2 class="text-lg font-semibold text-gray-900 mb-4">
                  Task Information
                </h2>

                <div class="space-y-4">
                  <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">
                      Status
                    </label>
                    <div class="flex items-center space-x-2">
                      <span
                        class={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${
                          task()!.status === "completed"
                            ? "bg-green-100 text-green-800"
                            : task()!.status === "running"
                              ? "bg-blue-100 text-blue-800"
                              : task()!.status === "failed"
                                ? "bg-red-100 text-red-800"
                                : "bg-gray-100 text-gray-800"
                        }`}
                      >
                        {task()!.status}
                      </span>
                    </div>
                  </div>

                  <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">
                      Repository
                    </label>
                    <p class="text-sm text-gray-900">
                      {task()!.repo.url || "Unknown"}
                    </p>
                  </div>

                  <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">
                      Branch
                    </label>
                    <p class="text-sm text-gray-900">
                      {task()!.repo.branch || "main"}
                    </p>
                  </div>

                  <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">
                      Agent
                    </label>
                    <p class="text-sm text-gray-900">
                      {task()!.agent.type} {task()!.agent.version}
                    </p>
                  </div>

                  <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">
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
              <div class="bg-white rounded-lg shadow-sm border border-gray-200">
                {/* Tab Navigation */}
                <div class="border-b border-gray-200">
                  <nav class="flex">
                    {[
                      { id: "overview", label: "Overview" },
                      { id: "logs", label: "Live Log" },
                      { id: "events", label: "Events" },
                      { id: "report", label: "Report" },
                      { id: "workspace", label: "Workspace" },
                    ].map((tab) => (
                      <button
                        onClick={() => setActiveTab(tab.id)}
                        class={`px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
                          activeTab() === tab.id
                            ? "border-blue-500 text-blue-600"
                            : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300"
                        }`}
                      >
                        {tab.label}
                      </button>
                    ))}
                  </nav>
                </div>

                {/* Tab Content */}
                <div class="p-6">
                  <Show when={activeTab() === "overview"}>
                    <div>
                      <h3 class="text-lg font-medium text-gray-900 mb-4">
                        Task Overview
                      </h3>
                      <div class="prose prose-sm max-w-none">
                        <p class="text-gray-700 whitespace-pre-wrap">
                          {task()!.prompt}
                        </p>
                      </div>
                    </div>
                  </Show>

                  <Show when={activeTab() === "logs"}>
                    <div>
                      <h3 class="text-lg font-medium text-gray-900 mb-4">
                        Live Log
                      </h3>
                      <div class="bg-gray-900 text-green-400 p-4 rounded-lg font-mono text-sm max-h-96 overflow-y-auto">
                        <Show when={logs().length > 0}>
                          <For each={logs()}>
                            {(log, index) => {
                              const logMessage = log.message || log;
                              const isError = logMessage.toLowerCase().includes('error') || logMessage.toLowerCase().includes('failed');
                              const isWarning = logMessage.toLowerCase().includes('warn') || logMessage.toLowerCase().includes('warning');

                              return (
                                <div class="mb-1">
                                  <span class="text-gray-400">
                                    [{new Date(task()!.createdAt).toLocaleTimeString()}]
                                  </span>
                                  <span
                                    class={`ml-2 ${
                                      isError
                                        ? 'text-red-400'
                                        : isWarning
                                          ? 'text-yellow-400'
                                          : 'text-green-400'
                                    }`}
                                  >
                                    {logMessage}
                                  </span>
                                </div>
                              );
                            }}
                          </For>
                        </Show>
                        <Show when={logs().length === 0}>
                          <div class="text-gray-500">
                            No logs available yet...
                          </div>
                        </Show>
                      </div>
                    </div>
                  </Show>

                  <Show when={activeTab() === "events"}>
                    <div>
                      <h3 class="text-lg font-medium text-gray-900 mb-4">
                        Events
                      </h3>
                      <div class="text-sm text-gray-500">
                        Events timeline will be displayed here...
                      </div>
                    </div>
                  </Show>

                  <Show when={activeTab() === "report"}>
                    <div>
                      <h3 class="text-lg font-medium text-gray-900 mb-4">
                        Report
                      </h3>
                      <div class="text-sm text-gray-500">
                        Task report and diff will be displayed here...
                      </div>
                    </div>
                  </Show>

                  <Show when={activeTab() === "workspace"}>
                    <div>
                      <h3 class="text-lg font-medium text-gray-900 mb-4">
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
          <div class="text-center py-12">
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
  );
};
