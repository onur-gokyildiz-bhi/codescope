import { createSignal, createMemo, createEffect, For, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import {
  projectVersion, setErrorMsg, setCenterNode, setViewMode, setSelectedNode,
} from '../store';
import { api } from '../api';
import {
  FileText, AlertCircle, CheckCircle2, Tag, Users,
  Search, Clock, ExternalLink, Lightbulb,
} from 'lucide-solid';
import { renderMarkdown } from '../utils/markdown';

interface Item {
  id: string;
  name: string;
  body: string;
  kind: string;
  timestamp?: string;
  file_path?: string;
  qualified_name?: string;
  category: CategoryId;
}

type CategoryId =
  | 'decisions'
  | 'problems'
  | 'solutions'
  | 'topics'
  | 'sessions'
  | 'code_decisions'
  | 'code_discussions';

const CATEGORIES: { id: CategoryId; label: string; Icon: any; color: string }[] = [
  { id: 'decisions',       label: 'Decisions',        Icon: Lightbulb,   color: 'var(--accent-violet)' },
  { id: 'problems',        label: 'Problems',         Icon: AlertCircle, color: 'var(--accent-magenta)' },
  { id: 'solutions',       label: 'Solutions',        Icon: CheckCircle2,color: 'var(--accent-lime)' },
  { id: 'topics',          label: 'Topics',           Icon: Tag,         color: 'var(--accent-amber)' },
  { id: 'sessions',        label: 'Sessions',         Icon: Users,       color: 'var(--text-dim)' },
  { id: 'code_decisions',  label: 'Code decisions',   Icon: Lightbulb,   color: 'var(--accent-violet)' },
  { id: 'code_discussions',label: 'Code discussions', Icon: FileText,    color: 'var(--text-dim)' },
];

function formatTimestamp(ts?: string): string {
  if (!ts) return '';
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return ts;
    return d.toISOString().slice(0, 16).replace('T', ' ');
  } catch {
    return ts;
  }
}

function excerpt(body: string, max = 160): string {
  const s = (body || '').replace(/```[\s\S]*?```/g, '…').replace(/\s+/g, ' ').trim();
  if (s.length <= max) return s;
  return s.slice(0, max).trimEnd() + '…';
}

export default function ArchiveView() {
  const [buckets, setBuckets] = createSignal<Record<CategoryId, Item[]>>({
    decisions: [], problems: [], solutions: [], topics: [],
    sessions: [], code_decisions: [], code_discussions: [],
  });
  const [activeCat, setActiveCat] = createSignal<CategoryId | 'all'>('decisions');
  const [query, setQuery] = createSignal('');
  const [selected, setSelected] = createSignal<Item | null>(null);
  const [loading, setLoading] = createSignal(true);

  createEffect(async () => {
    projectVersion();
    setLoading(true);
    setSelected(null);
    try {
      const data: any = await api.conversations();
      const out: Record<CategoryId, Item[]> = {
        decisions: [], problems: [], solutions: [], topics: [],
        sessions: [], code_decisions: [], code_discussions: [],
      };
      for (const cat of CATEGORIES) {
        const arr = (data?.[cat.id] || []) as any[];
        out[cat.id] = arr.map((c, idx) => ({
          id: c.qualified_name || c.id || `${cat.id}-${idx}`,
          name: c.name || c.title || c.summary || '(untitled)',
          body: c.body || c.content || '',
          kind: c.kind || cat.label,
          timestamp: c.timestamp,
          file_path: c.file_path,
          qualified_name: c.qualified_name,
          category: cat.id,
        }));
      }
      setBuckets(out);
      // Auto-pick first non-empty category if current is empty
      const cat = activeCat();
      if (cat !== 'all' && out[cat].length === 0) {
        const firstFull = CATEGORIES.find((c) => out[c.id].length > 0);
        if (firstFull) setActiveCat(firstFull.id);
      }
    } catch (e) {
      setErrorMsg(`Failed to load archive: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  });

  const categoryCount = (id: CategoryId) => buckets()[id].length;

  const allItems = createMemo<Item[]>(() => {
    const all: Item[] = [];
    for (const cat of CATEGORIES) all.push(...buckets()[cat.id]);
    return all.sort((a, b) => (b.timestamp || '').localeCompare(a.timestamp || ''));
  });

  const visibleItems = createMemo<Item[]>(() => {
    const cat = activeCat();
    const src = cat === 'all' ? allItems() : buckets()[cat];
    const q = query().trim().toLowerCase();
    const filtered = q
      ? src.filter((it) => (it.name + ' ' + it.body).toLowerCase().includes(q))
      : src;
    return [...filtered].sort((a, b) => (b.timestamp || '').localeCompare(a.timestamp || ''));
  });

  createEffect(() => {
    const list = visibleItems();
    if (selected() && list.find((it) => it.id === selected()!.id)) return;
    if (list.length > 0) setSelected(list[0]);
    else setSelected(null);
  });

  function showInGraph(item: Item) {
    setViewMode('graph');
    setCenterNode(item.name);
    setSelectedNode(null);
  }

  function categoryMeta(id: string) {
    return CATEGORIES.find((c) => c.id === id);
  }

  return (
    <div class="archive-view">
      {/* Sidebar */}
      <aside class="archive-sidebar">
        <div class="archive-sidebar-header">Archive</div>
        <button
          class={`archive-cat ${activeCat() === 'all' ? 'active' : ''}`}
          onClick={() => setActiveCat('all')}
        >
          <FileText size={13} stroke-width={1.75} />
          <span class="archive-cat-label">All</span>
          <span class="archive-cat-count">{allItems().length}</span>
        </button>
        <For each={CATEGORIES}>
          {(cat) => (
            <button
              class={`archive-cat ${activeCat() === cat.id ? 'active' : ''}`}
              onClick={() => setActiveCat(cat.id)}
              disabled={categoryCount(cat.id) === 0}
              style={{ '--cat-color': cat.color } as any}
            >
              <cat.Icon size={13} stroke-width={1.75} style={{ color: cat.color }} />
              <span class="archive-cat-label">{cat.label}</span>
              <span class="archive-cat-count">{categoryCount(cat.id)}</span>
            </button>
          )}
        </For>
      </aside>

      {/* Middle: item list */}
      <section class="archive-list">
        <div class="archive-search">
          <Search size={13} stroke-width={1.75} />
          <input
            type="text"
            class="archive-search-input"
            placeholder="Search archive…"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
          <Show when={query()}>
            <button class="archive-search-clear" onClick={() => setQuery('')} aria-label="Clear search">×</button>
          </Show>
        </div>
        <div class="archive-list-body">
          <Show when={loading()}>
            <div class="archive-empty">Loading…</div>
          </Show>
          <Show when={!loading() && visibleItems().length === 0}>
            <div class="archive-empty">
              <Show when={query()} fallback="No entries">No matches for "{query()}"</Show>
            </div>
          </Show>
          <For each={visibleItems()}>
            {(item) => {
              const cat = categoryMeta(item.category);
              const Icon = cat?.Icon;
              return (
                <button
                  class={`archive-item ${selected()?.id === item.id ? 'active' : ''}`}
                  onClick={() => setSelected(item)}
                >
                  <div class="archive-item-head">
                    <Show when={Icon}>
                      <Dynamic component={Icon!} size={11} stroke-width={1.75} style={{ color: cat!.color, 'flex-shrink': 0 }} />
                    </Show>
                    <span class="archive-item-title">{item.name}</span>
                  </div>
                  <div class="archive-item-meta">
                    <Show when={item.timestamp}>
                      <Clock size={10} stroke-width={1.75} />
                      <span>{formatTimestamp(item.timestamp)}</span>
                    </Show>
                    <Show when={item.file_path}>
                      <span class="archive-item-file">· {item.file_path!.split(/[\\/]/).pop()}</span>
                    </Show>
                  </div>
                  <div class="archive-item-excerpt">{excerpt(item.body)}</div>
                </button>
              );
            }}
          </For>
        </div>
      </section>

      {/* Right: detail */}
      <article class="archive-detail">
        <Show when={selected()} fallback={<div class="archive-empty">Select an entry</div>}>
          {(item) => {
            const cat = createMemo(() => categoryMeta(item().category));
            const CatIcon = createMemo(() => cat()?.Icon);
            return (
              <>
                <header class="archive-detail-head">
                  <div class="archive-detail-kind" style={{ color: cat()?.color }}>
                    <Show when={CatIcon()}>
                      <Dynamic component={CatIcon()!} size={14} stroke-width={1.75} />
                    </Show>
                    <span>{cat()?.label || item().kind}</span>
                  </div>
                  <h2 class="archive-detail-title">{item().name}</h2>
                  <div class="archive-detail-meta">
                    <Show when={item().timestamp}>
                      <span><Clock size={11} stroke-width={1.75} /> {formatTimestamp(item().timestamp)}</span>
                    </Show>
                    <Show when={item().file_path}>
                      <span>· source: <code>{item().file_path}</code></span>
                    </Show>
                    <button class="archive-show-in-graph" onClick={() => showInGraph(item())}>
                      <ExternalLink size={11} stroke-width={1.75} /> Show in graph
                    </button>
                  </div>
                </header>
                <div
                  class="archive-detail-body markdown-body"
                  innerHTML={renderMarkdown(item().body)}
                />
              </>
            );
          }}
        </Show>
      </article>
    </div>
  );
}
