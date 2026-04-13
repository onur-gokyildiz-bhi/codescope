import { For } from 'solid-js';
import { colorMode } from '../store';
import { KIND_COLORS } from '../utils/colors';

const CODE_KINDS = ['function', 'class', 'file', 'caller', 'callee', 'sibling'];
const KNOWLEDGE_KINDS: { kind: string; label: string }[] = [
  { kind: 'knowledge:concept', label: 'concept' },
  { kind: 'knowledge:entity', label: 'entity' },
  { kind: 'knowledge:decision', label: 'decision' },
  { kind: 'knowledge:source', label: 'source' },
];

export default function Legend() {
  return (
    <div class="legend glass" style="position:absolute;bottom:8px;left:8px;z-index:40">
      <For each={CODE_KINDS}>
        {(kind) => (
          <div class="legend-item">
            <span class="legend-dot" style={`background:${KIND_COLORS[kind] || '#8b949e'}`} />
            <span>{kind}</span>
          </div>
        )}
      </For>
      <span style="color:var(--border);margin:0 4px">|</span>
      <For each={KNOWLEDGE_KINDS}>
        {(k) => (
          <div class="legend-item">
            <span class="legend-dot" style={`background:${KIND_COLORS[k.kind] || '#F0883E'};border-radius:2px`} />
            <span>{k.label}</span>
          </div>
        )}
      </For>
      <div class="legend-item" style="margin-left:8px;opacity:0.6">
        {colorMode() === 'module' ? '(module colors)' : '(type colors)'}
      </div>
    </div>
  );
}
