import { createSignal, createEffect, For, Show } from 'solid-js';
import { setShowConv, projectVersion } from '../store';
import { api } from '../api';

type TabId = 'decisions' | 'problems' | 'solutions';

interface ConvItem {
  title: string;
  body: string;
  kind: string;
}

export default function ConvPanel() {
  const [tab, setTab] = createSignal<TabId>('decisions');
  const [items, setItems] = createSignal<ConvItem[]>([]);
  const [loading, setLoading] = createSignal(true);

  createEffect(async () => {
    projectVersion(); // re-fetch on project switch
    setLoading(true);
    try {
      const data = await api.conversations();
      const all: ConvItem[] = (data || []).map((c: any) => ({
        title: c.title || c.summary || '',
        body: c.body || c.content || '',
        kind: c.kind || c.type || 'decisions',
      }));
      setItems(all);
    } catch { /* ignore */ }
    setLoading(false);
  });

  function filtered(): ConvItem[] {
    return items().filter(i => i.kind === tab() || i.kind === tab().slice(0, -1));
  }

  const TABS: { id: TabId; label: string }[] = [
    { id: 'decisions', label: 'Decisions' },
    { id: 'problems', label: 'Problems' },
    { id: 'solutions', label: 'Solutions' },
  ];

  return (
    <>
      <div class="panel-header">
        <span>Conversations</span>
        <button class="panel-close" onClick={() => setShowConv(false)}>&times;</button>
      </div>
      <div class="conv-tabs">
        <For each={TABS}>
          {(t) => (
            <button
              class={`conv-tab ${tab() === t.id ? 'active' : ''}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          )}
        </For>
      </div>
      <div class="panel-body" style="max-height:200px;overflow-y:auto">
        <Show when={loading()}>
          <span style="color:var(--text-dim);font-size:12px">Loading...</span>
        </Show>
        <Show when={!loading() && filtered().length === 0}>
          <span style="color:var(--text-dim);font-size:12px">No {tab()} found</span>
        </Show>
        <For each={filtered()}>
          {(item) => (
            <div class="conv-item">
              <div class="conv-item-title">{item.title}</div>
              <div class="conv-item-body">{item.body}</div>
            </div>
          )}
        </For>
      </div>
    </>
  );
}
