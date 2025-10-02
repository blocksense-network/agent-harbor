import {
  Component,
  createSignal,
  onMount,
  onCleanup,
  createEffect,
  For,
} from "solid-js";
import TomSelect from "tom-select";

interface ModelSelection {
  model: string;
  instances: number;
}

interface ModelMultiSelectProps {
  availableModels: string[];
  selectedModels: ModelSelection[];
  onSelectionChange: (selections: ModelSelection[]) => void;
  placeholder?: string;
  testId?: string;
  class?: string;
}

// Type for Tom Select option data
interface TomSelectOptionData {
  text: string;
  value: string;
}

export const ModelMultiSelect: Component<ModelMultiSelectProps> = (props) => {
  let selectRef: HTMLSelectElement | undefined;
  let tomSelectInstance: TomSelect | undefined;
  const [localSelections, setLocalSelections] = createSignal<ModelSelection[]>(
    props.selectedModels || [],
  );

  // Track instance counts for ALL models (selected or not) to preserve dropdown increments
  const instanceCounts = new Map<string, number>();

  // Initialize counts from props
  props.selectedModels?.forEach((s) => {
    instanceCounts.set(s.model, s.instances);
  });

  const getInstanceCount = (model: string): number => {
    return instanceCounts.get(model) || 1;
  };

  const updateInstanceCount = (model: string, delta: number) => {
    const currentCount = getInstanceCount(model);
    const newCount = Math.max(1, Math.min(10, currentCount + delta));
    instanceCounts.set(model, newCount);

    // If the model is already selected, update localSelections
    const isSelected = localSelections().some((s) => s.model === model);
    if (isSelected) {
      const updated = localSelections().map((s) => {
        if (s.model === model) {
          return { ...s, instances: newCount };
        }
        return s;
      });
      setLocalSelections(updated);
      props.onSelectionChange(updated);
    }
  };

  const removeModel = (model: string) => {
    if (tomSelectInstance) {
      tomSelectInstance.removeItem(model, true);
    }
    // Reset count to 1 when model is removed (optional - could also delete from Map)
    instanceCounts.set(model, 1);
    const updated = localSelections().filter((s) => s.model !== model);
    setLocalSelections(updated);
    props.onSelectionChange(updated);
  };

  onMount(() => {
    if (!selectRef || typeof window === "undefined") return;

    // Initialize TOM Select with custom templates
    tomSelectInstance = new TomSelect(selectRef, {
      create: false,
      maxItems: null,
      placeholder: props.placeholder || "Select models...",
      searchField: ["text"],
      maxOptions: 100,
      plugins: [],
      dropdownParent: "body",

      onChange: (values: string[]) => {
        // Update selections, preserving instance counts from the Map
        const newSelections = values.map((model) => {
          return {
            model,
            instances: getInstanceCount(model), // Use Map instead of looking up in localSelections
          };
        });
        setLocalSelections(newSelections);
        props.onSelectionChange(newSelections);
      },

      render: {
        // Custom option template (dropdown) - ALWAYS show +/- buttons
        option: (
          data: TomSelectOptionData,
          escape: (str: string) => string,
        ) => {
          const model = escape(data.text);
          const count = getInstanceCount(data.text);

          const div = document.createElement("div");
          div.className =
            "flex items-center justify-between p-2 hover:bg-gray-50";
          div.innerHTML = `
            <span class="flex-1 text-sm">${model}</span>
            <div class="flex items-center gap-1 model-counter" data-model="${model}">
              <button type="button" class="decrease-btn w-6 h-6 flex items-center justify-center bg-white border border-gray-300 rounded text-gray-700 hover:bg-gray-100 text-sm font-bold" data-action="decrease">−</button>
              <span class="count-display w-8 text-center text-sm font-medium">${count}</span>
              <button type="button" class="increase-btn w-6 h-6 flex items-center justify-center bg-white border border-gray-300 rounded text-gray-700 hover:bg-gray-100 text-sm font-bold" data-action="increase">+</button>
            </div>
          `;

          // Attach event handlers to the buttons
          setTimeout(() => {
            const decreaseBtn = div.querySelector(".decrease-btn");
            const increaseBtn = div.querySelector(".increase-btn");
            const countDisplay = div.querySelector(".count-display");

            if (decreaseBtn && increaseBtn && countDisplay) {
              decreaseBtn.addEventListener("click", (e) => {
                e.preventDefault();
                e.stopPropagation();
                updateInstanceCount(data.text, -1);
                // Update display immediately
                const newCount = Math.max(1, count - 1);
                countDisplay.textContent = newCount.toString();
              });

              increaseBtn.addEventListener("click", (e) => {
                e.preventDefault();
                e.stopPropagation();
                updateInstanceCount(data.text, 1);
                // Update display immediately
                const newCount = Math.min(10, count + 1);
                countDisplay.textContent = newCount.toString();
              });
            }
          }, 0);

          return div;
        },

        // Custom item template (selected badge) - overlay +/- buttons
        item: (data: TomSelectOptionData, escape: (str: string) => string) => {
          const model = escape(data.text);
          const count = getInstanceCount(data.text);

          const div = document.createElement("div");
          div.className =
            "relative inline-flex items-center gap-1 px-2 py-1 bg-blue-50 border border-blue-200 rounded text-sm";
          div.setAttribute("data-value", data.value);
          div.innerHTML = `
            <span class="model-label">${model}</span>
            <span class="count-badge text-xs font-semibold text-blue-700 bg-blue-100 px-1.5 py-0.5 rounded">×${count}</span>
            <div class="badge-controls flex items-center gap-0.5 ml-1">
              <button type="button" class="decrease-badge-btn w-4 h-4 flex items-center justify-center bg-white border border-blue-300 rounded text-blue-700 hover:bg-blue-100 text-xs font-bold leading-none" data-action="decrease">−</button>
              <button type="button" class="increase-badge-btn w-4 h-4 flex items-center justify-center bg-white border border-blue-300 rounded text-blue-700 hover:bg-blue-100 text-xs font-bold leading-none" data-action="increase">+</button>
              <button type="button" class="remove-badge-btn w-4 h-4 flex items-center justify-center text-red-600 hover:bg-red-50 rounded text-xs font-bold leading-none ml-0.5" data-action="remove">×</button>
            </div>
          `;

          // Attach event handlers
          setTimeout(() => {
            const decreaseBtn = div.querySelector(".decrease-badge-btn");
            const increaseBtn = div.querySelector(".increase-badge-btn");
            const removeBtn = div.querySelector(".remove-badge-btn");
            const countBadge = div.querySelector(".count-badge");

            if (decreaseBtn && increaseBtn && removeBtn && countBadge) {
              decreaseBtn.addEventListener("click", (e) => {
                e.preventDefault();
                e.stopPropagation();
                const newCount = Math.max(1, count - 1);
                updateInstanceCount(data.text, -1);
                countBadge.textContent = `×${newCount}`;
              });

              increaseBtn.addEventListener("click", (e) => {
                e.preventDefault();
                e.stopPropagation();
                const newCount = Math.min(10, count + 1);
                updateInstanceCount(data.text, 1);
                countBadge.textContent = `×${newCount}`;
              });

              removeBtn.addEventListener("click", (e) => {
                e.preventDefault();
                e.stopPropagation();
                removeModel(data.text);
              });
            }
          }, 0);

          return div;
        },
      },
    });

    // Workaround: prevent dropdown translucency overlapping footer in dark mode
    if (tomSelectInstance.dropdown) {
      tomSelectInstance.dropdown.style.backgroundColor = "#ffffff";
      tomSelectInstance.dropdown.style.border = "1px solid #cccccc";
      tomSelectInstance.dropdown.style.borderRadius = "4px";
      tomSelectInstance.dropdown.style.boxShadow =
        "0 2px 8px rgba(0, 0, 0, 0.1)";
      tomSelectInstance.dropdown.style.zIndex = "9999";
    }

    // Set initial values if provided
    if (props.selectedModels && props.selectedModels.length > 0) {
      tomSelectInstance.setValue(
        props.selectedModels.map((s) => s.model),
        true,
      );
      setLocalSelections(props.selectedModels);
    }
  });

  // Re-render items when instance counts change
  createEffect(() => {
    if (!tomSelectInstance) return;

    const selections = localSelections();
    // Force TOM Select to re-render items by refreshing
    selections.forEach((selection) => {
      const item = tomSelectInstance!.getItem(selection.model);
      if (item) {
        const countBadge = item.querySelector(".count-badge");
        if (countBadge) {
          countBadge.textContent = `×${selection.instances}`;
        }
      }
    });
  });

  onCleanup(() => {
    tomSelectInstance?.destroy();
  });

  return (
    <div
      data-testid={props.testId}
      class={`
        model-multi-select
        ${props.class || ""}
      `}
    >
      <select
        ref={selectRef}
        class="tom-select-input"
        multiple
        aria-label={props.placeholder}
      >
        <For each={props.availableModels}>
          {(model) => <option value={model}>{model}</option>}
        </For>
      </select>
    </div>
  );
};
