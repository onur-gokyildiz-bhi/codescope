import { createSignal, createEffect, For, Show } from 'solid-js';
import { Tabs } from '@kobalte/core/tabs';
import { setShowConv, projectVersion, setErrorMsg } from '../store';
import { api } from '../api';
import { X } from 'lucide-solid';

type TabId = 'decisions' | 'problems' | 'solutions' | 'topics' | 'sessions';

interface ConvItem {
  name: string;
  body: string;
  kind: string;
  timestamp?: string;
  file_path?: string;
}

export default function ConvPanel() {
  const [tab, setTab] = createSignal<TabId>('decisions');
  const [items, setItems] = createSignal<Record<string, ConvItem[]>>({});
  const [loading, setLoading] = createSignal(true);

  createEffect(async () => {
    projectVersion();
    setLoading(true);
    try {
      const data = await api.conversations();
      const parsed: Record<string, ConvItem[]> = {};
      for (const key of ['decisions', 'problems', 'solutions', 'topics', 'sessions']) {
        parsed[key] = (data?.[key] || []).map((c: any) => ({
          name: c.name || c.title || c.summary || '',
          body: c.body || c.content || '',
          kind: c.kind || key,
          timestamp: c.timestamp || '',
          file_path: c.file_path || '',
        }));
      }
      setItems(parsed);
    } catch (e) { setErrorMsg(`Failed to load conversations: ${String(e)}`); }
    setLoading(false);
  });

  function filtered(): ConvItem[] {
    return items()[tab()] || [];
  }

  const TABS: { id: TabId; label: string }[] = [
    { id: 'decisions', label: 'Decisions' },
    { id: 'problems', label: 'Problems' },
    { id: 'solutions', label: 'Solutions' },
    { id: 'topics', label: 'Topics' },
    { id: 'sessions', label: 'Sessions' },
  ];

  return (
    <>
      <div class="panel-header">
        <span>Conversations</span>
        <button class="panel-close" onClick={() => setShowConv(false)} aria-label="Close conversations">
          <X size={14} stroke-width={1.75} />
        </button>
      </div>
      <Tabs value={tab()} onChange={(v) => setTab(v as TabId)}>
        <Tabs.List class="conv-tabs" style={{ 'overflow-x': 'auto', 'scrollbar-width': 'none' }}>
          <For each={TABS}>
            {(t) => (
              <Tabs.Trigger value={t.id} class="conv-tab">
                {t.label} ({(items()[t.id] || []).length})
              </Tabs.Trigger>
            )}
          </For>
          <Tabs.Indicator class="conv-tab-indicator" />
        </Tabs.List>
        <For each={TABS}>
          {(t) => (
            <Tabs.Content value={t.id}>
              <div class="panel-body" style="max-height:200px;overflow-y:auto">
                <Show when={loading()}>
                  <span style="color:var(--text-dim);font-size:12px">Loading...</span>
                </Show>
                <Show when={!loading() && filtered().length === 0}>
                  <span style="color:var(--text-dim);font-size:12px">No {t.id} found</span>
                </Show>
                <For each={filtered()}>
                  {(item) => (
                    <div class="conv-item">
                      <div class="conv-item-title">{item.name}</div>
                      <Show when={item.timestamp}>
                        <span style="color:var(--text-dim);font-size:10px;margin-left:8px">{item.timestamp}</span>
                      </Show>
                      <div class="conv-item-body">{item.body?.slice(0, 300)}{item.body?.length > 300 ? '...' : ''}</div>
                    </div>
                  )}
                </For>
              </div>
            </Tabs.Content>
          )}
        </For>
      </Tabs>
    </>
  );
}
