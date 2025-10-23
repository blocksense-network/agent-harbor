import { Title, Meta } from '@solidjs/meta';

export default function Settings() {
  return (
    <>
      <Title>Agent Harbor — Settings</Title>
      <Meta
        name="description"
        content="Configure Agent Harbor settings including tenant configuration, RBAC, API keys, and IDE integration"
      />
      <div class="flex-1 p-6">
        <div class="mx-auto max-w-4xl">
          <h1 class="mb-6 text-2xl font-bold text-gray-900">Settings</h1>
          <div class="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
            <p class="text-gray-600">Settings panel will be implemented here.</p>
            <p class="mt-2 text-sm text-gray-500">
              This will include tenant configuration, RBAC, API keys, and IDE integration settings.
            </p>
          </div>
        </div>
      </div>
    </>
  );
}
