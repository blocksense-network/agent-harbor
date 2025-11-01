/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createSignal, For, Show } from 'solid-js';

import { Icon } from './Icon';

// TODO: Use @apply directive
const countIconClass =
  'aspect-square bg-white border border-[#E2E8F0] rounded-[110px] cursor-pointer';
const countTextClass = 'text-[#004DFF] font-manrope text-[12px] font-medium leading-normal';

type ModelCountOutputProps = {
  label: string;
  count: number;
};

const ModelCountOutput = (props: ModelCountOutputProps) => (
  <output aria-label={`${props.label} selected count`} aria-live="polite" class={countTextClass}>
    {props.count}
  </output>
);

type ModelItemProps = {
  model: Model;
  increment: (model: Model) => void;
  decrement: (model: Model) => void;
  reset: (model: Model) => void;
};

const ModelBadge = (props: ModelItemProps) => {
  return (
    <li
      class={`
        flex items-center gap-1 rounded-[100px] border border-[#E2E8F0]
        bg-[#F5F6F9] p-1
      `}
    >
      <Icon
        as="button"
        variant="minus"
        wrapperSize="sm"
        aria-label="Decrease model count"
        onClick={() => props.decrement(props.model)}
        class={`
          ${countIconClass}
          p-1
        `}
      />
      <ModelCountOutput label={props.model.model} count={props.model.count} />
      <Icon
        as="button"
        variant="plus"
        wrapperSize="sm"
        aria-label="Increase model count"
        onClick={() => props.increment(props.model)}
        class={`
          ${countIconClass}
          p-1
        `}
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
        aria-label="Reset model count"
        onClick={() => props.reset(props.model)}
        class="cursor-pointer rounded-[100px] border border-[#FA6060]"
      />
    </li>
  );
};

const ModelOption = (props: ModelItemProps) => {
  return (
    <li
      role="option"
      aria-selected={props.model.count > 0 ? 'true' : 'false'}
      class={`
        group flex items-center gap-2 bg-white p-2
        first:rounded-t-lg
        last:rounded-b-lg
        hover:bg-[#F5F6F9]
      `}
    >
      <Icon
        as="button"
        variant="minus"
        wrapperSize="md"
        aria-label="Decrease model count"
        onClick={() => props.decrement(props.model)}
        class={`
          ${countIconClass}
          p-2
        `}
      />
      <ModelCountOutput label={props.model.model} count={props.model.count} />
      <Icon
        as="button"
        variant="plus"
        wrapperSize="md"
        aria-label="Increase model count"
        onClick={() => props.increment(props.model)}
        class={`
          ${countIconClass}
          p-2
        `}
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
};

type Model = {
  count: number;
  model: string;
};

export const ModelsDropdown = () => {
  const [isOpen, setIsOpen] = createSignal(false);
  const [models, setModels] = createSignal<Model[]>([
    { count: 0, model: 'Claude 3.5 Sonnet' },
    { count: 0, model: 'Claude 3.5' },
    { count: 0, model: 'Claude 3' },
  ]);

  const toggleDropdown = () => {
    setIsOpen(!isOpen());
  };

  const incrementModelCount = (model: Model) => {
    setModels(prevModels =>
      prevModels.map(m => (m.model === model.model ? { ...m, count: m.count + 1 } : m)),
    );
  };

  const decrementModelCount = (model: Model) => {
    if (model.count > 0) {
      setModels(prevModels =>
        prevModels.map(m => (m.model === model.model ? { ...m, count: m.count - 1 } : m)),
      );
    }
  };

  const resetModelCount = (model: Model) => {
    setModels(prevModels =>
      prevModels.map(m => (m.model === model.model ? { ...m, count: 0 } : m)),
    );
  };

  const selectedModels = () => models().filter(model => model.count > 0);
  const isModelSelected = () => selectedModels().length > 0;

  return (
    <section aria-label="Model selection" class="relative w-fit min-w-[431px]">
      <div
        classList={{
          'items-start': isModelSelected(),
          'items-center': !isModelSelected(),
        }}
        class={`
          flex gap-2 rounded-lg border border-[#D4DAE3] bg-white px-3 py-1
        `}
        role="group"
        aria-label="Selected models"
      >
        <Icon aria-hidden="true" variant="models-picker" wrapperSize="md" />
        <ul
          class="m-0 flex flex-1 list-none flex-wrap items-center gap-1 p-0"
          aria-label="Selected models"
        >
          <Show
            when={isModelSelected()}
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
          aria-label={isOpen() ? 'Hide model list' : 'Show model list'}
          aria-expanded={isOpen() ? true : false}
          aria-haspopup="listbox"
          onClick={toggleDropdown}
          class="ml-auto transition-transform duration-200"
          classList={{
            'rotate-180': isOpen(),
          }}
        />
      </div>
      {isOpen() && (
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
      )}
    </section>
  );
};
