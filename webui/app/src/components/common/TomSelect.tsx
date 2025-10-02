import { onMount, onCleanup, createEffect, For } from "solid-js";
import TomSelect from "tom-select";

interface TomSelectProps<T = string> {
  items: T[];
  selectedItem?: T | null;
  onSelect: (item: T | null) => void;
  getDisplayText: (item: T) => string;
  getKey: (item: T) => string;
  placeholder?: string;
  class?: string;
  testId?: string;
  id?: string;
}

export const TomSelectComponent = <T,>(props: TomSelectProps<T>) => {
  let selectRef: HTMLSelectElement | undefined;
  let tomSelectInstance: TomSelect | undefined;

  onMount(() => {
    if (!selectRef || typeof window === "undefined") return;

    // Initialize TOM Select with proper positioning and styling
    tomSelectInstance = new TomSelect(selectRef, {
      create: false,
      maxItems: 1,
      placeholder: props.placeholder || "Select...",
      searchField: ["text"],
      maxOptions: 100,
      onChange: (value: string) => {
        const item = props.items.find((item) => props.getKey(item) === value);
        props.onSelect(item || null);
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

    // Set initial value if provided
    if (props.selectedItem) {
      tomSelectInstance.setValue(props.getKey(props.selectedItem), true);
    }
  });

  // Update options when items change
  createEffect(() => {
    if (tomSelectInstance && typeof window !== "undefined") {
      tomSelectInstance!.clearOptions();

      props.items.forEach((item) => {
        tomSelectInstance!.addOption({
          value: props.getKey(item),
          text: props.getDisplayText(item),
        });
      });

      tomSelectInstance!.refreshOptions(false);
    }
  });

  // Update selected value when it changes
  createEffect(() => {
    if (tomSelectInstance && typeof window !== "undefined") {
      const newValue = props.selectedItem
        ? props.getKey(props.selectedItem)
        : "";
      if (tomSelectInstance!.getValue() !== newValue) {
        tomSelectInstance!.setValue(newValue, true);
      }
    }
  });

  onCleanup(() => {
    tomSelectInstance?.destroy();
  });

  return (
    <div class={props.class} data-testid={props.testId}>
      <select
        ref={selectRef}
        id={props.id}
        class="tom-select-input"
        aria-label={props.placeholder}
      >
        <option value="">{props.placeholder || "Select..."}</option>
        <For each={props.items}>
          {(item) => (
            <option value={props.getKey(item)}>
              {props.getDisplayText(item)}
            </option>
          )}
        </For>
      </select>
    </div>
  );
};
