import {
  createSignal,
  createEffect,
  onMount,
  onCleanup,
  For,
  Show,
} from "solid-js";

interface PopupSelectorProps<T> {
  items: T[];
  selectedItem: T | null;
  onSelect: (item: T) => void;
  getDisplayText: (item: T) => string;
  getKey: (item: T) => string;
  placeholder: string;
  icon?: string;
  class?: string;
}

export function PopupSelector<T>(props: PopupSelectorProps<T>) {
  const [isOpen, setIsOpen] = createSignal(false);
  const [searchTerm, setSearchTerm] = createSignal("");
  const [selectedIndex, setSelectedIndex] = createSignal(0);

  let buttonRef: HTMLButtonElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let listRef: HTMLUListElement | undefined;

  // Filter items based on search term (fuzzy matching)
  const filteredItems = () => {
    const term = searchTerm().toLowerCase();
    if (!term) return props.items;

    return props.items.filter((item) =>
      props.getDisplayText(item).toLowerCase().includes(term),
    );
  };

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    if (!isOpen()) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((prev) =>
          Math.min(prev + 1, filteredItems().length - 1),
        );
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((prev) => Math.max(prev - 1, 0));
        break;
      case "Enter": {
        e.preventDefault();
        const items = filteredItems();
        const selectedItem = items[selectedIndex()];
        if (selectedItem) {
          handleSelect(selectedItem);
        }
        break;
      }
      case "Escape":
        e.preventDefault();
        setIsOpen(false);
        buttonRef?.focus();
        break;
    }
  };

  const handleSelect = (item: T) => {
    props.onSelect(item);
    setIsOpen(false);
    setSearchTerm("");
    setSelectedIndex(0);
    buttonRef?.focus();
  };

  const handleButtonClick = () => {
    setIsOpen(!isOpen());
    if (!isOpen()) {
      setSearchTerm("");
      setSelectedIndex(0);
      // Focus input when opening
      setTimeout(() => inputRef?.focus(), 0);
    }
  };

  // Close popup when clicking outside
  const handleClickOutside = (e: MouseEvent) => {
    const path = e.composedPath();
    if (
      buttonRef &&
      listRef &&
      !path.includes(buttonRef) &&
      !path.includes(listRef)
    ) {
      setIsOpen(false);
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("click", handleClickOutside);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
    document.removeEventListener("click", handleClickOutside);
  });

  // Reset selected index when filtered items change
  createEffect(() => {
    filteredItems();
    setSelectedIndex(0);
  });

  return (
    <div class="relative">
      <button
        ref={buttonRef}
        onClick={handleButtonClick}
        class={`
          flex items-center space-x-1 rounded-md border px-3 py-1 text-sm
          transition-colors
          ${props.class || ""}
        `}
        classList={{
          "border-blue-300 bg-blue-50 text-blue-700": !!props.selectedItem,
          "border-gray-300 text-gray-700 hover:bg-gray-50": !props.selectedItem,
        }}
        aria-haspopup="listbox"
        aria-expanded={isOpen()}
        aria-label={`Select ${props.placeholder.toLowerCase()}`}
      >
        <Show when={props.icon}>
          <span>{props.icon}</span>
        </Show>
        <span>
          {props.selectedItem
            ? props.getDisplayText(props.selectedItem)
            : props.placeholder}
        </span>
        <span class="ml-1">▼</span>
      </button>

      <Show when={isOpen()}>
        <div
          class={`
            absolute z-50 mt-1 w-80 rounded-md border border-gray-200 bg-white
            shadow-lg
          `}
        >
          {/* Search input */}
          <div class="border-b border-gray-200 p-2">
            <input
              ref={inputRef}
              type="text"
              value={searchTerm()}
              onInput={(e) => setSearchTerm(e.currentTarget.value)}
              placeholder={`Search ${props.placeholder.toLowerCase()}...`}
              class={`
                w-full rounded border border-gray-300 px-3 py-2 text-sm
                focus:ring-2 focus:ring-blue-500 focus:outline-none
              `}
            />
          </div>

          {/* Items list */}
          <ul ref={listRef} role="listbox" class="max-h-60 overflow-y-auto">
            <For each={filteredItems()}>
              {(item, index) => (
                <li
                  role="option"
                  aria-selected={index() === selectedIndex()}
                  class={`
                    cursor-pointer px-3 py-2 text-sm
                    hover:bg-gray-100
                  `}
                  classList={{
                    "bg-blue-50 text-blue-700": index() === selectedIndex(),
                  }}
                  onClick={() => handleSelect(item)}
                  onMouseEnter={() => setSelectedIndex(index())}
                >
                  {props.getDisplayText(item)}
                </li>
              )}
            </For>

            <Show when={filteredItems().length === 0}>
              <li class="px-3 py-2 text-center text-sm text-gray-500">
                No {props.placeholder.toLowerCase()} found
              </li>
            </Show>
          </ul>
        </div>
      </Show>
    </div>
  );
}
