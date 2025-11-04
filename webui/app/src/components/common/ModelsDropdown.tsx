/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createEffect, createMemo, createSignal, For, Show, onCleanup } from 'solid-js';

import { Icon } from './Icon';

type Model = {
  count: number;
  model: string;
};

type ModelItemHandlers = {
  increment: (model: Model) => void;
  decrement: (model: Model) => void;
  reset: (model: Model) => void;
};

type ModelItemProps = {
  model: Model;
} & ModelItemHandlers;

type CountButtonProps = {
  model: Model;
  size: 'sm' | 'md';
} & Pick<ModelItemHandlers, 'increment' | 'decrement'>;

// TODO: Use @apply directive
const countIconClass = 'bg-white border border-[#E2E8F0] rounded-[110px] cursor-pointer';

const CountButtons = (props: CountButtonProps) => (
  <>
    <Icon
      as="button"
      variant="minus"
      wrapperSize={props.size}
      type="button"
      aria-label="Decrease model count"
      iconProps={{ 'aria-hidden': 'true' }}
      onClick={() => props.decrement(props.model)}
      wrapperClass={`${countIconClass} ${props.size === 'sm' ? 'p-1' : 'p-2'}`}
    />
    <output
      aria-label={`${props.model.count} selected count`}
      aria-live="polite"
      class={`
        font-manrope text-[12px] leading-normal font-medium text-[#004DFF]
      `}
    >
      {props.model.count}
    </output>
    <Icon
      as="button"
      variant="plus"
      wrapperSize={props.size}
      type="button"
      aria-label="Increase model count"
      iconProps={{ 'aria-hidden': 'true' }}
      onClick={() => props.increment(props.model)}
      wrapperClass={`${countIconClass} ${props.size === 'sm' ? 'p-1' : 'p-2'}`}
    />
  </>
);

const ModelBadge = (props: ModelItemProps) => (
  <li
    class={`
      flex items-center gap-1 rounded-[100px] border border-[#E2E8F0]
      bg-[#F5F6F9] p-1
    `}
  >
    <CountButtons
      model={props.model}
      size="sm"
      increment={props.increment}
      decrement={props.decrement}
    />
    <span
      class={`
        text-[12px] leading-normal font-semibold tracking-[-0.24px]
        text-[#4A5868]
      `}
    >
      {props.model.model}
    </span>
    <Icon
      as="button"
      variant="close"
      wrapperSize="sm"
      type="button"
      aria-label="Reset model count"
      iconProps={{ 'aria-hidden': 'true' }}
      onClick={() => props.reset(props.model)}
      wrapperClass="cursor-pointer rounded-[100px] border border-[#FA6060]"
    />
  </li>
);

const ModelOption = (props: ModelItemProps) => (
  <li
    role="option"
    aria-selected={props.model.count > 0}
    class={`
      group flex items-center gap-2 bg-white p-2
      first:rounded-t-lg
      last:rounded-b-lg
      hover:bg-[#F5F6F9]
    `}
  >
    <CountButtons
      model={props.model}
      size="md"
      increment={props.increment}
      decrement={props.decrement}
    />
    <span
      class={`
        text-[14px] leading-normal font-medium text-[#4B5563CC]
        group-hover:font-bold group-hover:text-[#4A5868]
      `}
    >
      {props.model.model}
    </span>
  </li>
);

export const ModelsDropdown = () => {
  const [isOpen, setIsOpen] = createSignal(false);
  const [models, setModels] = createSignal<Model[]>([
    { count: 0, model: 'Claude 3.5 Sonnet' },
    { count: 0, model: 'Claude 3.5' },
    { count: 0, model: 'Claude 3' },
  ]);
  let dropdownRef: Element;

  const updateModel = (name: string, updater: (count: number) => number) => {
    setModels(prev =>
      prev.map(m => (m.model === name ? { ...m, count: Math.max(0, updater(m.count)) } : m)),
    );
  };
  const incrementModelCount = (model: Model) => updateModel(model.model, c => c + 1);
  const decrementModelCount = (model: Model) => updateModel(model.model, c => c - 1);
  const resetModelCount = (model: Model) => updateModel(model.model, () => 0);

  const selectedModels = createMemo(() => models().filter(model => model.count > 0));
  const hasSelectedModels = createMemo(() => selectedModels().length > 0);

  const toggleDropdown = () => {
    setIsOpen(prev => !prev);
  };

  createEffect(() => {
    if (!isOpen()) return;

    const handleClickOutside = (event: MouseEvent) => {
      if (!dropdownRef) return;
      const path = event.composedPath();
      if (!path.includes(dropdownRef)) {
        setIsOpen(false);
      }
    };

    window.addEventListener('click', handleClickOutside);
    onCleanup(() => window.removeEventListener('click', handleClickOutside));
  });

  return (
    <section
      ref={el => {
        dropdownRef = el;
      }}
      aria-label="Model selection"
      class="relative w-fit min-w-[431px]"
    >
      <div
        classList={{
          'items-start': hasSelectedModels(),
          'items-center': !hasSelectedModels(),
        }}
        class="flex gap-2 rounded-lg border border-[#D4DAE3] bg-white px-3 py-1"
        role="group"
        aria-label="Selected models"
      >
        <Icon aria-hidden="true" variant="models" as="div" wrapperSize="md" />
        <ul
          class="m-0 flex flex-1 list-none flex-wrap items-center gap-1 p-0"
          aria-label="Selected models"
        >
          <Show
            when={hasSelectedModels()}
            fallback={
              <li
                class={`
                  text-[14px] leading-normal font-medium tracking-[-0.14px]
                  text-[#4A5868]
                `}
                aria-hidden="true"
              >
                Model
              </li>
            }
          >
            <For each={selectedModels()}>
              {model => (
                <ModelBadge
                  model={model}
                  increment={incrementModelCount}
                  decrement={decrementModelCount}
                  reset={resetModelCount}
                />
              )}
            </For>
          </Show>
        </ul>
        <Icon
          as="button"
          variant="arrow-down"
          wrapperSize="md"
          type="button"
          aria-label={isOpen() ? 'Hide model list' : 'Show model list'}
          aria-expanded={isOpen()}
          aria-haspopup="listbox"
          onClick={toggleDropdown}
          iconProps={{ 'aria-hidden': 'true' }}
          wrapperClass="ml-auto transition-transform duration-200"
          wrapperClassList={{
            'rotate-180': isOpen(),
          }}
        />
      </div>
      <Show when={isOpen()}>
        <ul
          class={`
            absolute right-0 left-0 z-10 mt-1 list-none rounded-lg border
            border-[#D4DAE3] bg-white p-0
          `}
          aria-label="Available models"
          role="listbox"
        >
          <For
            each={models()}
            fallback={
              <li
                class={`
                  p-2 text-[14px] leading-normal font-medium tracking-[-0.14px]
                  text-[#4A5868]
                `}
              >
                No models found
              </li>
            }
          >
            {model => (
              <ModelOption
                model={model}
                increment={incrementModelCount}
                decrement={decrementModelCount}
                reset={resetModelCount}
              />
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
};
