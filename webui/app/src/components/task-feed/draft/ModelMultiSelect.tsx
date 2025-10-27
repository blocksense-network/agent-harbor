/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import TomSelect from 'tom-select';

export type ModelSelection = {
  model: string;
  instances: number;
};

type ModelMultiSelectProps = {
  availableModels: string[];
  selectedModels: ModelSelection[];
  onSelectionChange: (selections: ModelSelection[]) => void;
  placeholder?: string;
  testId?: string;
  class?: string;
};

const MIN_SELECTED_INSTANCES = 1;
const MAX_INSTANCES = 10;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

type CountsMap = Record<string, number>;

type ModelAction = 'increase' | 'decrease';
type BadgeAction = ModelAction | 'remove';

type OptionTemplateData = {
  value: string;
  label: string;
  count?: number;
};

const modelKey = (value: string) => encodeURIComponent(value.toLowerCase());

declare module 'tom-select' {
  interface TomSelectOption {
    count?: number;
  }

  interface TomSelect {
    /**
     * Clears cached render output to force Tom Select to use latest option data.
     * https://tom-select.js.org/docs/#methods-clearcache
     */
    clearCache(template?: 'item' | 'option'): void;
  }
}

export const ModelMultiSelect = (props: ModelMultiSelectProps) => {
  let selectRef: HTMLSelectElement | undefined;
  let tomSelect: TomSelect | undefined;
  let dropdownClickHandler: ((event: MouseEvent) => void) | undefined;
  let controlClickHandler: ((event: MouseEvent) => void) | undefined;
  let dropdownPointerDownHandler: ((event: MouseEvent) => void) | undefined;
  let controlPointerDownHandler: ((event: MouseEvent) => void) | undefined;
  let externalSyncKey = '';
  let lastNotifiedKey = '';

  const [counts, setCounts] = createSignal<CountsMap>({});

  const models = () => props.availableModels ?? [];
  const containerClass = () =>
    props.class && props.class.trim().length > 0 ? `relative ${props.class}` : 'relative';

  const updateTomSelectOption = (model: string, count: number) => {
    if (!tomSelect) {
      return;
    }

    const existing = tomSelect.options[model];
    const payload = {
      value: model,
      text: existing?.['text'] ?? model,
      label: (existing as { label?: string })?.label ?? model,
      count,
    };

    logDebug('updateOption', model, payload);
    tomSelect.updateOption(model, payload);
    if (tomSelect.options[model]) {
      Object.assign(tomSelect.options[model], payload);
    }
    if (typeof tomSelect.clearCache === 'function') {
      tomSelect.clearCache();
    }
  };

  const buildCountsMap = (selections: ModelSelection[] | undefined): CountsMap => {
    const base: CountsMap = {};
    for (const model of models()) {
      base[model] = 0;
    }

    if (selections) {
      for (const selection of selections) {
        if (selection.model in base) {
          base[selection.model] = clamp(selection.instances, MIN_SELECTED_INSTANCES, MAX_INSTANCES);
        }
      }
    }

    return base;
  };

  const notifyParent = (countsMap: CountsMap) => {
    const selections: ModelSelection[] = [];
    for (const model of models()) {
      const count = countsMap[model] ?? 0;
      if (count > 0) {
        selections.push({ model, instances: count });
      }
    }

    const key = JSON.stringify(selections);
    if (key === lastNotifiedKey) {
      return;
    }

    lastNotifiedKey = key;
    props.onSelectionChange(selections);
  };

  const commitCounts = (
    nextCounts: CountsMap,
    options: { notifyParent?: boolean; manageSelection?: boolean } = {},
  ) => {
    externalSyncKey = JSON.stringify(nextCounts);
    setCounts(nextCounts);

    if (tomSelect) {
      for (const model of models()) {
        const count = nextCounts[model] ?? 0;
        updateTomSelectOption(model, count);
      }

      if (options.manageSelection !== false) {
        const selectedValues = models().filter(model => (nextCounts[model] ?? 0) > 0);

        for (const value of [...tomSelect.items]) {
          if (!selectedValues.includes(value)) {
            tomSelect.removeItem(value, true);
          }
        }

        for (const value of selectedValues) {
          if (!tomSelect.items.includes(value)) {
            tomSelect.addItem(value, true);
          }
        }
      }

      tomSelect.refreshItems();
      tomSelect.refreshOptions();
      logDebug('refreshUI', {
        items: [...tomSelect.items],
        dropdownItems: Object.keys(nextCounts),
      });
    }

    if (options.notifyParent !== false) {
      notifyParent(nextCounts);
    }
  };

  const applyCountChange = (
    model: string,
    desired: number,
    options: {
      min?: number;
      notifyParent?: boolean;
      manageSelection?: boolean;
    } = {},
  ) => {
    const current = counts();
    const min = options.min ?? 0;
    const clampedValue = clamp(desired, min, MAX_INSTANCES);

    if ((current[model] ?? 0) === clampedValue) {
      return;
    }

    const nextCounts: CountsMap = { ...current, [model]: clampedValue };
    commitCounts(nextCounts, options);
  };

  const handleDropdownClick = (event: MouseEvent) => {
    if (!tomSelect) {
      return;
    }

    const target = event.target;
    if (!(target instanceof globalThis.HTMLElement)) {
      return;
    }

    const button = target.closest<HTMLButtonElement>('[data-model-action]');

    if (!button) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    const value = button.getAttribute('data-value');
    const actionAttr = button.getAttribute('data-model-action');

    if (!value || !actionAttr) {
      return;
    }

    const action = actionAttr as ModelAction;
    const currentCount = counts()[value] ?? 0;
    if (action === 'increase') {
      const next = Math.min(currentCount + 1, MAX_INSTANCES);
      applyCountChange(value, next, {
        notifyParent: false,
      });
      logDebug('dropdown increase', value, { currentCount, next });
      tomSelect.open(); // keep dropdown visible when adjusting counts in-place
    } else if (action === 'decrease') {
      const next = Math.max(currentCount - 1, 0);
      applyCountChange(value, next, {
        notifyParent: false,
      });
      logDebug('dropdown decrease', value, { currentCount, next });
      tomSelect.open(); // re-open in case Tom Select processed the click as a selection
    }
  };

  const handleBadgeClick = (event: MouseEvent) => {
    if (!tomSelect) {
      return;
    }

    const target = event.target;
    if (!(target instanceof globalThis.HTMLElement)) {
      return;
    }

    const button = target.closest<HTMLButtonElement>('[data-badge-action]');

    if (!button) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    const value = button.getAttribute('data-value');
    const actionAttr = button.getAttribute('data-badge-action');

    if (!value || !actionAttr) {
      return;
    }

    const action = actionAttr as BadgeAction;
    const currentCount = counts()[value] ?? MIN_SELECTED_INSTANCES;
    if (action === 'increase') {
      const next = Math.min(currentCount + 1, MAX_INSTANCES);
      applyCountChange(value, next, {
        min: MIN_SELECTED_INSTANCES,
        notifyParent: true,
      });
      logDebug('badge increase', value, { currentCount, next });
      return;
    }

    if (action === 'decrease') {
      const next = Math.max(currentCount - 1, MIN_SELECTED_INSTANCES);
      applyCountChange(value, next, {
        min: MIN_SELECTED_INSTANCES,
        notifyParent: true,
      });
      logDebug('badge decrease', value, { currentCount, next });
      return;
    }

    if (action === 'remove') {
      applyCountChange(value, 0, {
        notifyParent: true,
      });
      logDebug('badge remove', value);
    }
  };

  // Guard against Tom Select interpreting button presses as option selections.
  const preventTomSelectSelection = (event: MouseEvent) => {
    const target = event.target;
    if (!(target instanceof globalThis.HTMLElement)) {
      return;
    }

    const interactiveElement = target.closest('[data-model-action], [data-badge-action]');

    if (interactiveElement) {
      event.preventDefault();
      event.stopPropagation();
    }
  };

  onMount(() => {
    const initialCounts = buildCountsMap(props.selectedModels);
    setCounts(initialCounts);

    tomSelect = new TomSelect(selectRef!, {
      options: models().map(model => ({
        value: model,
        label: model,
        count: initialCounts[model] ?? 0,
      })),
      items: models().filter(model => (initialCounts[model] ?? 0) > 0),
      placeholder: props.placeholder ?? 'Models',
      valueField: 'value',
      labelField: 'label',
      searchField: ['label'],
      maxItems: null,
      closeAfterSelect: false,
      render: {
        option: (data: OptionTemplateData, escape: (value: string) => string) => {
          const count = counts()[data.value] ?? 0;
          const decreaseDisabled = count <= 0 ? 'disabled' : '';
          const key = modelKey(data.value);
          return `
            <div class="flex items-center justify-between gap-3">
              <span class="model-label">${escape(data.label)}</span>
              <div class="model-counter flex items-center gap-1" data-model-key="${key}">
                <button
                  type="button"
                  class="decrease-btn flex h-6 w-6 items-center justify-center rounded border border-gray-300 bg-white text-sm font-bold text-gray-700"
                  data-model-action="decrease"
                  data-value="${escape(data.value)}"
                  aria-label="Decrease ${escape(data.label)} instances"
                  ${decreaseDisabled}
                >
                  −
                </button>
                <span class="count-display w-8 text-center text-sm font-medium" data-role="count">${count}</span>
                <button
                  type="button"
                  class="increase-btn flex h-6 w-6 items-center justify-center rounded border border-gray-300 bg-white text-sm font-bold text-gray-700"
                  data-model-action="increase"
                  data-value="${escape(data.value)}"
                  aria-label="Increment ${escape(data.label)} instances"
                >
                  +
                </button>
              </div>
            </div>
          `;
        },
        item: (data: OptionTemplateData, escape: (value: string) => string) => {
          const count = counts()[data.value] ?? MIN_SELECTED_INSTANCES;
          const key = modelKey(data.value);
          return `
            <div class="model-badge item inline-flex items-center gap-1 rounded border border-blue-200 bg-blue-50 px-2 py-1 text-sm" data-model-key="${key}">
              <span class="model-label">${escape(data.label)}</span>
              <span class="count-badge rounded bg-blue-100 px-1.5 py-0.5 text-xs font-semibold text-blue-700">×${count}</span>
              <div class="badge-controls ml-1 flex items-center gap-0.5">
                <button
                  type="button"
                  class="decrease-badge-btn flex h-4 w-4 items-center justify-center rounded border border-blue-300 bg-white text-xs font-bold leading-none text-blue-700"
                  data-badge-action="decrease"
                  data-value="${escape(data.value)}"
                  aria-label="Decrease ${escape(data.label)} instances"
                >
                  −
                </button>
                <button
                  type="button"
                  class="increase-badge-btn flex h-4 w-4 items-center justify-center rounded border border-blue-300 bg-white text-xs font-bold leading-none text-blue-700"
                  data-badge-action="increase"
                  data-value="${escape(data.value)}"
                  aria-label="Increment ${escape(data.label)} instances"
                >
                  +
                </button>
                <button
                  type="button"
                  class="remove-badge-btn ml-0.5 flex h-4 w-4 items-center justify-center rounded text-xs font-bold leading-none text-red-600"
                  data-badge-action="remove"
                  data-value="${escape(data.value)}"
                  aria-label="Remove ${escape(data.label)}"
                >
                  ×
                </button>
              </div>
            </div>
          `;
        },
      },
    });

    dropdownClickHandler = handleDropdownClick;
    controlClickHandler = handleBadgeClick;
    dropdownPointerDownHandler = preventTomSelectSelection;
    controlPointerDownHandler = preventTomSelectSelection;

    tomSelect.dropdown_content?.addEventListener('click', dropdownClickHandler);
    tomSelect.control?.addEventListener('click', controlClickHandler);
    tomSelect.dropdown_content?.addEventListener('pointerdown', dropdownPointerDownHandler, true);
    tomSelect.dropdown_content?.addEventListener('mousedown', dropdownPointerDownHandler, true);
    tomSelect.control?.addEventListener('pointerdown', controlPointerDownHandler, true);
    tomSelect.control?.addEventListener('mousedown', controlPointerDownHandler, true);

    tomSelect['on']('item_add', (value: string) => {
      const current = counts()[value] ?? 0;
      const next = current > 0 ? current : MIN_SELECTED_INSTANCES;
      applyCountChange(value, next, {
        min: MIN_SELECTED_INSTANCES,
        manageSelection: false,
      });
    });

    tomSelect['on']('item_remove', (value: string) => {
      applyCountChange(value, 0, {
        manageSelection: false,
      });
    });

    if (selectRef) {
      selectRef.removeAttribute('hidden');
      selectRef.classList.remove('ts-hidden-accessible');
      selectRef.style.display = 'block';
      selectRef.style.position = 'absolute';
      selectRef.style.opacity = '0.01';
      selectRef.style.pointerEvents = 'none';
      selectRef.style.height = '1px';
      selectRef.style.width = '1px';
    }

    commitCounts(initialCounts, { notifyParent: false });
  });

  onCleanup(() => {
    if (tomSelect) {
      if (dropdownClickHandler) {
        const dropdownEl = tomSelect.dropdown_content;
        dropdownEl?.removeEventListener('click', dropdownClickHandler);
      }

      if (controlClickHandler) {
        const controlEl = tomSelect.control;
        controlEl?.removeEventListener('click', controlClickHandler);
      }

      if (dropdownPointerDownHandler) {
        const dropdownEl = tomSelect.dropdown_content;
        dropdownEl?.removeEventListener('pointerdown', dropdownPointerDownHandler, true);
        dropdownEl?.removeEventListener('mousedown', dropdownPointerDownHandler, true);
      }

      if (controlPointerDownHandler) {
        const controlEl = tomSelect.control;
        controlEl?.removeEventListener('pointerdown', controlPointerDownHandler, true);
        controlEl?.removeEventListener('mousedown', controlPointerDownHandler, true);
      }

      tomSelect.destroy();
      tomSelect = undefined;
    }
  });

  createEffect(() => {
    if (!tomSelect) {
      return;
    }

    const nextCounts = buildCountsMap(props.selectedModels);
    const key = JSON.stringify(nextCounts);

    if (key !== externalSyncKey) {
      commitCounts(nextCounts, { notifyParent: false });
    }
  });

  return (
    <div data-testid={props.testId} class={containerClass()}>
      <select ref={selectRef} multiple class="tom-select-input">
        <For each={models()}>{model => <option value={model}>{model}</option>}</For>
      </select>
    </div>
  );
};

const logDebug = (...args: unknown[]) => console.debug('[ModelMultiSelect]', ...args);
