import { createSignal, createEffect, onMount, For, Show } from 'solid-js';
import { currentFile, setCurrentFile } from '../store';
import { api } from '../api';

declare const hljs: any;

export default function SourceViewer() {
  const [content, setContent] = createSignal<string>('');
  const [entities, setEntities] = createSignal<any[]>([]);
  let codeRef!: HTMLDivElement;

  createEffect(async () => {
    const path = currentFile();
    if (!path) return;
    try {
      const data = await api.fileContent(path);
      setContent(data.content || '');
      setEntities(data.entities || []);
    } catch {
      setContent('// Failed to load file');
      setEntities([]);
    }
  });

  function close() {
    setCurrentFile(null);
  }

  function scrollToLine(line: number) {
    const el = codeRef?.querySelector(`[data-line="${line}"]`);
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  function highlightedLines(): string[] {
    const raw = content();
    if (!raw) return [];
    try {
      const result = hljs.highlightAuto(raw);
      return result.value.split('\n');
    } catch {
      return raw.split('\n');
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }

  onMount(() => {
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  });

  return (
    <div class="source-overlay">
      <div class="source-backdrop" onClick={close} />
      <div class="source-container panel">
        <div class="source-header">
          <span class="mono">{currentFile()}</span>
          <button class="panel-close" onClick={close}>&times;</button>
        </div>
        <div class="source-body">
          <div class="source-symbols">
            <For each={entities()}>
              {(ent) => (
                <div
                  class="source-symbol-item"
                  onClick={() => scrollToLine(ent.line || 1)}
                  title={`${ent.kind}: ${ent.name}`}
                >
                  <span style={`color:var(--accent);margin-right:4px`}>
                    {ent.kind === 'function' ? 'fn' : ent.kind === 'class' ? 'cls' : ent.kind?.slice(0, 3)}
                  </span>
                  {ent.name}
                </div>
              )}
            </For>
            <Show when={entities().length === 0}>
              <div class="source-symbol-item" style="color:var(--text-dim)">No symbols</div>
            </Show>
          </div>
          <div class="source-code" ref={codeRef}>
            <pre><code>
              <For each={highlightedLines()}>
                {(line, i) => (
                  <div class="source-line" data-line={i() + 1}>
                    <span class="source-line-num">{i() + 1}</span>
                    <span class="source-line-code" innerHTML={line || ' '} />
                  </div>
                )}
              </For>
            </code></pre>
          </div>
        </div>
      </div>
    </div>
  );
}
