import { createSignal, For } from 'solid-js';
import { KIND_COLORS } from '../utils/colors';

const KINDS = ['function', 'class', 'file', 'caller', 'callee', 'sibling'];

export default function FilterPanel() {
  const [enabled, setEnabled] = createSignal<Set<string>>(new Set(KINDS));

  function toggle(kind: string) {
    setEnabled(prev => {
      const next = new Set(prev);
      if (next.has(kind)) {
        next.delete(kind);
      } else {
        next.add(kind);
      }
      return next;
    });
  }

  return (
    <div class="panel" style="width:180px">
      <div class="panel-header">
        <span>Filter</span>
      </div>
      <div class="panel-body">
        <For each={KINDS}>
          {(kind) => (
            <label class="filter-item" onClick={() => toggle(kind)}>
              <span
                class="filter-color"
                style={`background:${KIND_COLORS[kind] || '#8b949e'};opacity:${enabled().has(kind) ? 1 : 0.3}`}
              />
              <span style={`opacity:${enabled().has(kind) ? 1 : 0.5}`}>{kind}</span>
            </label>
          )}
        </For>
      </div>
    </div>
  );
}
