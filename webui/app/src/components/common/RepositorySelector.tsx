import {
  Component,
  createResource,
  Show,
  For,
  onMount,
  onCleanup,
  createEffect,
} from "solid-js";
import TomSelect from "tom-select";

interface Repository {
  id: string;
  name: string;
  branch: string;
  lastCommit: string;
}

// Type for Tom Select option data
interface RepositoryOptionData {
  name: string;
  branch: string;
}

interface RepositorySelectorProps {
  value?: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  class?: string;
  placeholder?: string;
}

export const RepositorySelector: Component<RepositorySelectorProps> = (
  props,
) => {
  let selectEl!: HTMLSelectElement;
  let ts: TomSelect | undefined;

  const [repositories] = createResource(async () => {
    // In a real implementation, this would fetch from the API
    // For now, return mock data
    const mockRepos: Repository[] = [
      { id: "1", name: "user/repo1", branch: "main", lastCommit: "abc123" },
      { id: "2", name: "user/repo2", branch: "develop", lastCommit: "def456" },
      { id: "3", name: "org/project1", branch: "main", lastCommit: "ghi789" },
      {
        id: "4",
        name: "org/project2",
        branch: "feature-branch",
        lastCommit: "jkl012",
      },
    ];
    return mockRepos;
  });

  onMount(() => {
    if (!selectEl) return;

    ts = new TomSelect(selectEl, {
      valueField: "name",
      labelField: "name",
      searchField: ["name"],
      placeholder: props.placeholder || "Select repository...",
      maxOptions: 100,
      closeAfterSelect: true,
      render: {
        option: (data: RepositoryOptionData, escape: (s: string) => string) => {
          return `<div class="flex items-center justify-between">
            <span>${escape(data.name)}</span>
            <span class="text-xs text-gray-500 ml-2">${escape(data.branch)}</span>
          </div>`;
        },
        item: (data: RepositoryOptionData, escape: (s: string) => string) => {
          return `<div class="flex items-center justify-between">
            <span>${escape(data.name)}</span>
            <span class="text-xs text-gray-500 ml-2">${escape(data.branch)}</span>
          </div>`;
        },
      },
    });

    // Set initial value if provided
    if (props.value) {
      ts.setValue(props.value);
    }

    // Listen for changes
    ts["on"]("change", (value: string) => {
      props.onChange(value);
    });
  });

  onCleanup(() => {
    ts?.destroy();
  });

  // Update Tom Select when value prop changes
  createEffect(() => {
    if (ts && props.value !== undefined) {
      ts.setValue(props.value);
    }
  });

  return (
    <select
      ref={selectEl}
      class={props.class}
      classList={{
        "cursor-not-allowed bg-gray-100": !!props.disabled,
      }}
      disabled={props.disabled}
    >
      <Show when={repositories()}>
        <For each={repositories()}>
          {(repo) => <option value={repo.name}>{repo.name}</option>}
        </For>
      </Show>
    </select>
  );
};
