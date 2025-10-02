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

interface BranchSelectorProps {
  repository: string; // Repository name like "user/repo"
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  class?: string;
}

interface Branch {
  name: string;
  isDefault?: boolean;
}

export const BranchSelector: Component<BranchSelectorProps> = (props) => {
  let selectEl!: HTMLSelectElement;
  let ts: TomSelect | undefined;

  // Mock branch data - in real implementation this would come from API
  const [branches] = createResource(
    () => props.repository,
    async (_repo) => {
      // Mock branches for demo - replace with actual API call
      const mockBranches: Branch[] = [
        { name: "main", isDefault: true },
        { name: "develop" },
        { name: "feature/new-ui" },
        { name: "feature/api-improvements" },
        { name: "bugfix/login-issue" },
        { name: "hotfix/security-patch" },
      ];

      return mockBranches;
    },
  );

  onMount(() => {
    if (!selectEl) return;

    ts = new TomSelect(selectEl, {
      valueField: "name",
      labelField: "name",
      searchField: ["name"],
      placeholder: "Select branch...",
      maxOptions: 50,
      closeAfterSelect: true,
      render: {
        option: (data: any, escape: (s: string) => string) => {
          return `
            <div class="flex items-center justify-between">
              <span>${escape(data.name)}</span>
              ${data.isDefault ? '<span class="text-xs text-gray-500 bg-gray-200 px-1 rounded">default</span>' : ""}
            </div>
          `;
        },
        item: (data: any, escape: (s: string) => string) => {
          return `
            <div class="flex items-center justify-between">
              <span>${escape(data.name)}</span>
              ${data.isDefault ? '<span class="text-xs text-gray-500 bg-gray-200 px-1 rounded">default</span>' : ""}
            </div>
          `;
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
      class={`
        ${props.class}
        ${props.disabled ? "cursor-not-allowed bg-gray-100" : ""}
      `}
      disabled={props.disabled}
    >
      <Show when={branches()}>
        <For each={branches()}>
          {(branch) => <option value={branch.name}>{branch.name}</option>}
        </For>
      </Show>
    </select>
  );
};
