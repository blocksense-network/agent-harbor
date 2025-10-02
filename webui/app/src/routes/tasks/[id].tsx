import { Component } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { Title, Meta } from "@solidjs/meta";

const TaskDetailsPage: Component = () => {
  const params = useParams();
  const navigate = useNavigate();
  const taskId = params["id"];

  const handleGoBack = () => {
    navigate("/", { replace: true });
  };

  return (
    <>
      <Title>Agent Harbor — Task {taskId}</Title>
      <Meta
        name="description"
        content={`View details and monitor progress for task ${taskId}`}
      />
      <div class="flex h-full flex-col p-4">
        <h2 class="mb-4 text-2xl font-bold">Task Details: {taskId}</h2>
        <div class="flex-1 rounded-lg border border-gray-200 bg-white p-6">
          <p class="text-gray-700">
            This is a placeholder for the task details page.
          </p>
          <p class="mt-2 text-gray-500">
            Details for task ID:{" "}
            <span class="font-mono text-blue-600">{taskId}</span> will be
            displayed here.
          </p>
          <p class="mt-2 text-gray-500">
            Press{" "}
            <kbd class="rounded border bg-gray-100 px-1 py-0.5 text-xs">
              Esc
            </kbd>{" "}
            to go back to the task feed.
          </p>
        </div>
        <button
          onClick={handleGoBack}
          class={`
            mt-4 rounded-md bg-blue-600 px-4 py-2 text-white
            hover:bg-blue-700
            focus:ring-2 focus:ring-blue-500 focus:outline-none
          `}
        >
          Go Back to Task Feed
        </button>
      </div>
    </>
  );
};

export default TaskDetailsPage;
