import { createSignal, For } from 'solid-js';

import modelsIcon from '../../assets/models.svg';
import arrowDown from '../../assets/arrow-down.svg';
import minusIcon from '../../assets/minus.svg';
import plusIcon from '../../assets/plus.svg';
import closeIcon from '../../assets/close.svg';

type CountIconProps = {
  variant: 'minus' | 'plus';
  onClick: () => void;
  isSmall?: boolean;
};

const CountIcon = (props: CountIconProps) => {
  const icons = {
    minus: minusIcon,
    plus: plusIcon,
  };

  return (
    <img
      src={icons[props.variant]}
      alt={props.variant}
      onClick={props.onClick}
      class="aspect-square p-2 bg-white border border-[#E2E8F0] rounded-[110px] cursor-pointer"
      classList={{
        'p-1': props.isSmall,
        'p-2': !props.isSmall,
      }}
    />
  );
};

const Count = (props: { count: number }) => {
  return (
    <span class="text-[#004DFF] font-manrope text-[12px] font-medium leading-normal">
      {props.count}
    </span>
  );
};

type ModelItemProps = {
  model: Model;
  increment: (model: Model) => void;
  decrement: (model: Model) => void;
  reset: (model: Model) => void;
};

const ModelBadge = (props: ModelItemProps) => {
  return (
    <div class="flex items-center gap-1 bg-[#F5F6F9] border border-[#E2E8F0] rounded-[100px] p-1">
      <CountIcon isSmall variant="minus" onClick={() => props.decrement(props.model)} />
      <Count count={props.model.count} />
      <CountIcon isSmall variant="plus" onClick={() => props.increment(props.model)} />
      <span class="text-[#4A5868] font-manrope text-[12px] font-semibold leading-normal tracking-[-0.24px]">
        {props.model.model}
      </span>
      <img
        src={closeIcon}
        alt="Close"
        onClick={() => props.reset(props.model)}
        class="cursor-pointer"
      />
    </div>
  );
};

const ModelOption = (props: ModelItemProps) => {
  return (
    <div class="flex items-center gap-2 p-2 bg-white hover:bg-[#F5F6F9] group first:rounded-t-lg last:rounded-b-lg">
      <CountIcon variant="minus" onClick={() => props.decrement(props.model)} />
      <Count count={props.model.count} />
      <CountIcon variant="plus" onClick={() => props.increment(props.model)} />
      <span class="text-[#4B5563CC] group-hover:text-[#4A5868] font-manrope text-[14px] font-medium group-hover:font-bold leading-normal">
        {props.model.model}
      </span>
    </div>
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
    <div class="relative min-w-[431px] w-fit">
      <div
        classList={{
          'items-start': isModelSelected(),
          'items-center': !isModelSelected(),
        }}
        class={
          'flex gap-2 rounded-lg border border-[#D4DAE3] bg-white px-3 py-1 [--count-icon-height:--spacing(2)] [--count-icon-padding:--spacing(1)] [--badge-y-padding:--spacing(1)]'
        }
      >
        <img
          src={modelsIcon}
          alt="Models"
          classList={{
            'mt-[calc(var(--count-icon-height)/2+var(--count-icon-padding)+var(--badge-y-padding)-8px)]':
              isModelSelected(),
          }}
        />
        <section class="flex gap-1 flex-wrap">
          <For
            each={selectedModels()}
            fallback={
              <span class="text-[#4A5868] font-manrope text-[14px] font-medium leading-normal tracking-[-0.14px]">
                Model
              </span>
            }
          >
            {model => (
              <ModelBadge
                model={model}
                increment={incrementModelCount}
                decrement={decrementModelCount}
                reset={resetModelCount}
              />
            )}
          </For>
        </section>
        <img
          src={arrowDown}
          alt="Arrow"
          onClick={toggleDropdown}
          class="ml-auto transition-transform duration-200"
          classList={{
            'mt-[calc((var(--count-icon-height)+2*var(--count-icon-padding)+2*var(--badge-y-padding)-16px)/2)]':
              isModelSelected(),
            'rotate-180': isOpen(),
          }}
        />
      </div>
      {isOpen() && (
        <div class="absolute left-0 right-0 z-10 mt-1 rounded-lg border border-[#D4DAE3] bg-white">
          <For
            each={models()}
            fallback={
              <span class="text-[#4A5868] font-manrope text-[14px] font-medium leading-normal tracking-[-0.14px]">
                No models found
              </span>
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
        </div>
      )}
    </div>
  );
};
