import { createSignal, createEffect, onMount, For, Show } from 'solid-js';
import { setShowPalette, setCenterNode, setSelectedNode } from '../store';
import { api } from '../api';
import { fuzzyScore } from '../utils/fuzzy';

interface PaletteItem {
  name: string;
  kind: string;
  confidence?: string;
  action: () => void;
}

const BUILT_IN: PaletteItem[] = [];

function isKnowledge(kind: string): boolean {
  return kind.startsWith('knowledge:');
}

export default function CommandPalette() {
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<PaletteItem[]>(BUILT_IN);
  const [selected, setSelected] = createSignal(0);
  let inputRef!: HTMLInputElement;
  let debounceTimer: number | undefined;

  onMount(() => inputRef.focus());

  createEffect(() => {
    const q = query();
    clearTimeout(debounceTimer);
    if (!q.trim()) {
      setResults(BUILT_IN);
      setSelected(0);
      return;
    }

    debounceTimer = window.setTimeout(async () => {
      try {
        const searchResults = await api.search(q);
        const items: PaletteItem[] = (searchResults || []).map((r: any) => ({
          name: r.name || r.id,
          kind: r.kind || 'function',
          confidence: r.confidence,
          action: () => {
            if (isKnowledge(r.kind || '')) {
              setSelectedNode({
                id: r.id,
                name: r.name || r.id,
                kind: r.kind,
                confidence: r.confidence,
                tags: r.tags,
              });
            } else {
              setCenterNode(r.name || r.id);
            }
          },
        }));

        const builtinMatches = BUILT_IN.filter(b => fuzzyScore(q, b.name) > 0);
        setResults([...builtinMatches, ...items]);
        setSelected(0);
      } catch {
        const builtinMatches = BUILT_IN.filter(b => fuzzyScore(q, b.name) > 0);
        setResults(builtinMatches);
      }
    }, 150);
  });

  function close() {
    setShowPalette(false);
  }

  function activate(item: PaletteItem) {
    item.action();
    close();
  }

  function onKeyDown(e: KeyboardEvent) {
    const list = results();
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected(i => Math.min(i + 1, list.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected(i => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = list[selected()];
      if (item) activate(item);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  }

  function kindLabel(kind: string): string {
    if (isKnowledge(kind)) return kind.split(':')[1] || 'knowledge';
    return kind;
  }

  function kindClass(kind: string): string {
    if (isKnowledge(kind)) return 'palette-item-kind knowledge';
    return 'palette-item-kind';
  }

  return (
    <div class="palette-overlay">
      <div class="palette-backdrop" onClick={close} />
      <div class="palette-box glass">
        <input
          ref={inputRef}
          class="palette-input"
          type="text"
          placeholder="Search functions, knowledge, files..."
          value={query()}
          onInput={e => setQuery(e.currentTarget.value)}
          onKeyDown={onKeyDown}
        />
        <div class="palette-results">
          <For each={results()}>
            {(item, i) => (
              <div
                class={`palette-item ${i() === selected() ? 'selected' : ''}`}
                onClick={() => activate(item)}
                onMouseEnter={() => setSelected(i())}
              >
                <span>{item.name}</span>
                <span class="palette-item-meta">
                  <Show when={item.confidence}>
                    <span class={`confidence-dot confidence-${item.confidence}`} />
                  </Show>
                  <span class={kindClass(item.kind)}>{kindLabel(item.kind)}</span>
                </span>
              </div>
            )}
          </For>
          <Show when={results().length === 0}>
            <div class="palette-item" style="color:var(--text-dim)">No results</div>
          </Show>
        </div>
      </div>
    </div>
  );
}
