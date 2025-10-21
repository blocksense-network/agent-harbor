You can implement quantity badges with Tom Select by (1) rendering your own option/item templates that include buttons and a quantity badge, (2) using event delegation to handle clicks on those buttons, and (3) listening for keyboard shortcuts to change the quantity of the **currently highlighted** option. Below is a Solid-friendly pattern you can drop in.

### Why this works

- **Templates**: Tom Select lets you override the HTML for options/items, so you can add `+`/`–` buttons and a qty badge. ([tom-select.js.org][1])
- **Events / API**: You can attach handlers after init and use the instance API (`refreshOptions`, etc.) to re-render the dropdown after you mutate option data. ([tom-select.js.org][2])

---

## SolidJS example

```tsx
import { onMount, onCleanup, createSignal } from 'solid-js';
import TomSelect from 'tom-select';

type Item = { value: string; label: string; qty: number };

export default function CartSelect() {
  let selectEl!: HTMLSelectElement;
  const [items, setItems] = createSignal<Item[]>([
    { value: 'apples', label: 'Apples', qty: 0 },
    { value: 'bananas', label: 'Bananas', qty: 0 },
    { value: 'pears', label: 'Pears', qty: 0 },
  ]);

  let ts: TomSelect | undefined;

  // helper: update qty in our source-of-truth and in Tom Select's option data
  function setQty(value: string, delta: number) {
    setItems(prev =>
      prev.map(it => (it.value === value ? { ...it, qty: Math.max(0, it.qty + delta) } : it)),
    );
    if (!ts) return;
    // mutate Tom Select's stored option data as well
    const opt = ts.options[value];
    if (opt) opt.qty = Math.max(0, (opt.qty ?? 0) + delta);

    // Force the dropdown option to re-render. If a browser cache prevents
    // an immediate re-render, update the active node directly as a fallback.
    ts.refreshOptions(false); // refresh list; keep it open
    const active = ts.dropdown_content?.querySelector<HTMLElement>('.active');
    const badge = active?.querySelector<HTMLElement>("[data-role='qty']");
    if (badge && opt) badge.textContent = String(opt.qty ?? 0);
  }

  onMount(() => {
    ts = new TomSelect(selectEl, {
      options: items().map(i => ({ value: i.value, label: i.label, qty: i.qty })),
      valueField: 'value',
      labelField: 'label',
      searchField: ['label'],
      // Custom templates: option rows with qty badge and +/– buttons
      render: {
        option: (data: any, escape: (s: string) => string) => {
          const qty = data.qty ?? 0;
          return `
            <div class="flex items-center justify-between gap-2">
              <span class="truncate">${escape(data.label)}</span>
              <div class="flex items-center gap-1">
                <button class="ts-btn ts-minus" data-action="dec" data-value="${escape(
                  data.value,
                )}" type="button" aria-label="Decrease">–</button>
                <span class="ts-qty" data-role="qty" aria-live="polite">${qty}</span>
                <button class="ts-btn ts-plus" data-action="inc" data-value="${escape(
                  data.value,
                )}" type="button" aria-label="Increase">+</button>
              </div>
            </div>
          `;
        },
        // Selected item chip shows the qty, too
        item: (data: any, escape: (s: string) => string) => {
          const qty = data.qty ?? 0;
          return `
            <div class="flex items-center gap-2">
              <span>${escape(data.label)}</span>
              <span class="ts-chip">${qty}</span>
            </div>
          `;
        },
      },
      // Keep dropdown open so users can spam +/–
      closeAfterSelect: false,
      // Optional: remove_button plugin if you also want an “x” on chips
      plugins: ['remove_button'],
    });

    // Click handlers via event delegation (works for virtualized rows, etc.)
    ts.on('initialize', () => {
      ts!.dropdown_content.addEventListener('click', e => {
        const btn = (e.target as HTMLElement).closest<HTMLButtonElement>('[data-action]');
        if (!btn) return;
        e.preventDefault();
        const value = btn.getAttribute('data-value')!;
        const action = btn.getAttribute('data-action');
        if (action === 'inc') setQty(value, +1);
        if (action === 'dec') setQty(value, -1);
      });
    });

    // Keyboard shortcuts on the control input:
    //   + / = : increment highlighted option
    //   -     : decrement highlighted option
    //   Shift+A: add highlighted option to selection
    ts.control_input.addEventListener('keydown', (e: KeyboardEvent) => {
      const active = ts!.activeOption; // currently highlighted option element
      if (!active) return;
      const value = active.getAttribute('data-value');
      if (!value) return;

      if (e.key === '+' || e.key === '=') {
        e.preventDefault();
        setQty(value, +1);
      } else if (e.key === '-') {
        e.preventDefault();
        setQty(value, -1);
      } else if (e.key.toLowerCase() === 'a' && e.shiftKey) {
        e.preventDefault();
        ts!.addItem(value); // select it
      }
    });
  });

  onCleanup(() => ts?.destroy());

  return (
    <div class="space-y-2">
      <label for="cart" class="block text-sm font-medium">
        Add items
      </label>
      <select id="cart" ref={selectEl} multiple />
      <style>{`
        .ts-btn { border: 1px solid #ddd; padding: 0 0.5rem; line-height: 1.5rem; border-radius: 0.375rem; }
        .ts-qty, .ts-chip { min-width: 1.25rem; text-align: center; font-variant-numeric: tabular-nums; }
        .ts-chip { background: #eee; border-radius: 9999px; padding: 0 0.4rem; }
      `}</style>
    </div>
  );
}
```

### Notes & gotchas

- **Re-rendering options**: Tom Select caches option DOM; calling `refreshOptions(false)` after you mutate `ts.options[value]` ensures the dropdown reflects the new qty. Some older versions didn’t re-render reliably from `updateOption` alone; the refresh call (and the tiny fallback that updates the `.active` node’s badge) avoids that edge case. ([Stack Overflow][3])
- **Where to listen**: Use the instance’s `initialize` event to add DOM listeners safely after Tom Select builds the dropdown, and the documented **events API** for general hooks. ([tom-select.js.org][2])
- **Templates**: Keep your option template a single root element and escape user strings. ([tom-select.js.org][1])
- **Solid lifecycle**: Always `destroy()` the instance on cleanup to prevent stale handlers. (API docs show instance methods and how to access the instance.) ([tom-select.js.org][4])

---

### Variations you might want

- Add a **“cart total”** signal and recompute from `items()` if you need a summary row.
- If the qty should also be visible on the **selected chips**, Tom Select will call your `render.item` when items are added. If the qty changes later, call `ts.refreshItems()` to rebuild item chips (same idea as options). ([tom-select.js.org][5])
- Prefer the **`clear_button`** plugin if you want a “clear all” affordance out of the box. It’s a good reference for writing custom mini-plugins, too. ([tom-select.js.org][6])

If you share your current init code (and whether you’re on Tom Select 2.x), I can adapt the snippet exactly to your setup and styling.

[1]: https://tom-select.js.org/docs/?utm_source=chatgpt.com 'Usage Documentation - Tom Select'
[2]: https://tom-select.js.org/docs/events/?utm_source=chatgpt.com 'Events API - Tom Select'
[3]: https://stackoverflow.com/questions/73890677/tomselect-refresh-options-in-dependent-dropdown-after-repeated-ajax-load?utm_source=chatgpt.com 'javascript - TomSelect - refresh options in dependent Dropdown after ...'
[4]: https://tom-select.js.org/docs/api/?utm_source=chatgpt.com 'API - Tom Select'
[5]: https://tom-select.js.org/examples/api/?utm_source=chatgpt.com 'JavaScript API Examples - Tom Select'
[6]: https://tom-select.js.org/plugins/clear-button/?utm_source=chatgpt.com 'Clear Button - Tom Select'
