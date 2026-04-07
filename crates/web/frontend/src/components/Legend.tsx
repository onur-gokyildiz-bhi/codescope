import { For } from 'solid-js';
import { colorMode } from '../store';
import { KIND_COLORS } from '../utils/colors';

const LEGEND_KINDS = ['function', 'class', 'file', 'caller', 'callee', 'sibling'];

export default function Legend() {
  return (
    <div class="legend glass" style="position:absolute;bottom:8px;left:8px;z-index:40">
      <For each={LEGEND_KINDS}>
        {(kind) => (
          <div class="legend-item">
            <span class="legend-dot" style={`background:${KIND_COLORS[kind] || '#8b949e'}`} />
            <span>{kind}</span>
          </div>
        )}
      </For>
      <div class="legend-item" style="margin-left:8px;opacity:0.6">
        {colorMode() === 'module' ? '(module colors)' : '(type colors)'}
      </div>
    </div>
  );
}
