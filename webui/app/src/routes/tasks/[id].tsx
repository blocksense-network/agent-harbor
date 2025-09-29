import { Component } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { Title, Meta } from "@solidjs/meta";

const TaskDetailsPage: Component = () => {
  const params = useParams();
  const navigate = useNavigate();
  const taskId = params.id;

  const handleGoBack = () => {
    navigate("/", { replace: true });
  };

  return (
    <>
      <Title>Agent Harbor — Task {taskId}</Title>
      <Meta name="description" content={`View details and monitor progress for task ${taskId}`} />
      <div class="flex flex-col h-full p-4">
        <h2 class="text-2xl font-bold mb-4">Task Details: {taskId}</h2>
      <div class="bg-white border border-gray-200 rounded-lg p-6 flex-1">
        <p class="text-gray-700">This is a placeholder for the task details page.</p>
        <p class="text-gray-500 mt-2">Details for task ID: <span class="font-mono text-blue-600">{taskId}</span> will be displayed here.</p>
        <p class="text-gray-500 mt-2">Press <kbd class="px-1 py-0.5 bg-gray-100 border rounded text-xs">Esc</kbd> to go back to the task feed.</p>
      </div>
      <button
        onClick={handleGoBack}
        class="mt-4 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
      >
        Go Back to Task Feed
      </button>
    </div>
    </>
  );
};

export default TaskDetailsPage;