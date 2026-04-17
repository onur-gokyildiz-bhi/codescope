import { createSignal, createMemo, createEffect, onMount, onCleanup, For, Show, batch } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import {
  projectVersion, setErrorMsg, setCenterNode, setViewMode, setSelectedNode,
} from '../store';
import { api } from '../api';
import {
  FileText, AlertCircle, CheckCircle2, Tag, Users, Brain,
  Search, Clock, ExternalLink, Lightbulb, Copy, Check,
  ArrowDownWideNarrow, ArrowUpNarrowWide, Hash,
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
  tags?: string[];
  confidence?: string;
  source_url?: string;
  category: CategoryId;
}

type CategoryId =
  | 'decisions'
  | 'problems'
  | 'solutions'
  | 'topics'
  | 'sessions'
  | 'knowledge';

type SortOrder = 'newest' | 'oldest';

const CATEGORIES: { id: CategoryId; label: string; Icon: any; color: string }[] = [
  { id: 'decisions', label: 'Decisions', Icon: Lightbulb,    color: 'var(--accent-violet)' },
  { id: 'problems',  label: 'Problems',  Icon: AlertCircle,  color: 'var(--accent-magenta)' },
  { id: 'solutions', label: 'Solutions', Icon: CheckCircle2, color: 'var(--accent-lime)' },
  { id: 'topics',    label: 'Topics',    Icon: Tag,          color: 'var(--accent-amber)' },
  { id: 'sessions',  label: 'Sessions',  Icon: Users,        color: 'var(--text-dim)' },
  { id: 'knowledge', label: 'Knowledge', Icon: Brain,        color: 'var(--accent-teal, #00e5ff)' },
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

function sourceName(fp?: string): string {
  if (!fp) return '';
  return fp.split(/[\\/]/).pop() || fp;
}

function itemToMarkdown(it: Item): string {
  const parts: string[] = [];
  parts.push(`# ${it.name}\n`);
  const meta: string[] = [`**${it.kind}**`];
  if (it.timestamp) meta.push(formatTimestamp(it.timestamp));
  if (it.file_path) meta.push(`source: \`${it.file_path}\``);
  if (it.confidence) meta.push(`confidence: ${it.confidence}`);
  parts.push(meta.join(' · ') + '\n');
  if (it.tags && it.tags.length) parts.push(`Tags: ${it.tags.map((t) => `\`${t}\``).join(' ')}\n`);
  parts.push('\n' + (it.body || ''));
  if (it.source_url) parts.push(`\n\n[source](${it.source_url})`);
  return parts.join('\n');
}

export default function ArchiveView() {
  const emptyBuckets = (): Record<CategoryId, Item[]> => ({
    decisions: [], problems: [], solutions: [], topics: [],
    sessions: [], knowledge: [],
  });

  const [buckets, setBuckets] = createSignal<Record<CategoryId, Item[]>>(emptyBuckets());
  const [activeCat, setActiveCat] = createSignal<CategoryId | 'all'>('decisions');
  const [query, setQuery] = createSignal('');
  const [activeTag, setActiveTag] = createSignal<string | null>(null);
  const [activeSource, setActiveSource] = createSignal<string | null>(null);
  const [sort, setSort] = createSignal<SortOrder>('newest');
  const [selected, setSelected] = createSignal<Item | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [copied, setCopied] = createSignal(false);

  let searchInputEl: HTMLInputElement | undefined;

  createEffect(async () => {
    projectVersion();
    setLoading(true);
    batch(() => {
      setSelected(null);
      setActiveTag(null);
      setActiveSource(null);
    });
    try {
      const data: any = await api.conversations();
      const out = emptyBuckets();
      for (const cat of CATEGORIES) {
        const arr = (data?.[cat.id] || []) as any[];
        out[cat.id] = arr.map((c, idx) => ({
          id: c.id?.id?.String || c.id?.String || c.qualified_name || c.id || `${cat.id}-${idx}`,
          name: c.name || c.title || c.summary || '(untitled)',
          body: c.body || c.content || '',
          kind: c.kind || cat.label,
          timestamp: c.timestamp,
          file_path: c.file_path,
          qualified_name: c.qualified_name,
          tags: Array.isArray(c.tags) ? c.tags : undefined,
          confidence: c.confidence,
          source_url: c.source_url,
          category: cat.id,
        }));
      }
      setBuckets(out);
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
    return all;
  });

  // Collect all tags for chip bar
  const allTags = createMemo<{ tag: string; count: number }[]>(() => {
    const counts = new Map<string, number>();
    for (const it of allItems()) {
      for (const t of it.tags || []) counts.set(t, (counts.get(t) || 0) + 1);
    }
    return [...counts.entries()]
      .map(([tag, count]) => ({ tag, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 20);
  });

  const visibleItems = createMemo<Item[]>(() => {
    const cat = activeCat();
    let src = cat === 'all' ? allItems() : buckets()[cat];
    const tag = activeTag();
    if (tag) src = src.filter((it) => (it.tags || []).includes(tag));
    const source = activeSource();
    if (source) src = src.filter((it) => it.file_path === source);
    const q = query().trim().toLowerCase();
    const filtered = q
      ? src.filter((it) => (it.name + ' ' + it.body).toLowerCase().includes(q))
      : src;
    const ord = sort();
    return [...filtered].sort((a, b) => {
      const cmp = (b.timestamp || '').localeCompare(a.timestamp || '');
      return ord === 'newest' ? cmp : -cmp;
    });
  });

  // Related entries from same source file
  const relatedItems = createMemo<Item[]>(() => {
    const sel = selected();
    if (!sel?.file_path) return [];
    return allItems()
      .filter((it) => it.file_path === sel.file_path && it.id !== sel.id)
      .sort((a, b) => (b.timestamp || '').localeCompare(a.timestamp || ''))
      .slice(0, 8);
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

  function copyAsMarkdown(item: Item) {
    const md = itemToMarkdown(item);
    try {
      navigator.clipboard.writeText(md).then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      });
    } catch {
      setErrorMsg('Clipboard write failed');
    }
  }

  function moveSelection(delta: number) {
    const list = visibleItems();
    if (list.length === 0) return;
    const cur = selected();
    const idx = cur ? list.findIndex((it) => it.id === cur.id) : -1;
    const next = Math.max(0, Math.min(list.length - 1, idx + delta));
    setSelected(list[next]);
    // Scroll selected item into view
    queueMicrotask(() => {
      document.querySelector('.archive-item.active')?.scrollIntoView({ block: 'nearest' });
    });
  }

  function clearFilters() {
    batch(() => {
      setQuery('');
      setActiveTag(null);
      setActiveSource(null);
    });
  }

  function handleKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const inInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA';

    // '/' focuses search from anywhere except input
    if (e.key === '/' && !inInput) {
      e.preventDefault();
      searchInputEl?.focus();
      return;
    }
    if (inInput) {
      if (e.key === 'Escape') {
        (document.activeElement as HTMLElement)?.blur();
      }
      return;
    }
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      moveSelection(1);
    } else if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      moveSelection(-1);
    } else if (e.key === 'Enter') {
      const sel = selected();
      if (sel) showInGraph(sel);
    } else if (e.key === 'Escape') {
      clearFilters();
    } else if (e.key === 'y' && !e.ctrlKey && !e.metaKey) {
      const sel = selected();
      if (sel) copyAsMarkdown(sel);
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKey);
    onCleanup(() => window.removeEventListener('keydown', handleKey));
  });

  const filterBadgeCount = () => {
    let n = 0;
    if (query()) n++;
    if (activeTag()) n++;
    if (activeSource()) n++;
    return n;
  };

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
            >
              <cat.Icon size={13} stroke-width={1.75} style={{ color: cat.color }} />
              <span class="archive-cat-label">{cat.label}</span>
              <span class="archive-cat-count">{categoryCount(cat.id)}</span>
            </button>
          )}
        </For>

        {/* Top tags */}
        <Show when={allTags().length > 0}>
          <div class="archive-sidebar-header" style="margin-top:12px">Tags</div>
          <div class="archive-tags-chips">
            <For each={allTags()}>
              {(t) => (
                <button
                  class={`archive-tag-chip ${activeTag() === t.tag ? 'active' : ''}`}
                  onClick={() => setActiveTag(activeTag() === t.tag ? null : t.tag)}
                  title={`Filter: ${t.tag}`}
                >
                  <Hash size={10} stroke-width={1.75} />
                  <span>{t.tag}</span>
                  <span class="archive-tag-count">{t.count}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </aside>

      {/* Middle: item list */}
      <section class="archive-list">
        <div class="archive-search">
          <Search size={13} stroke-width={1.75} />
          <input
            ref={searchInputEl}
            type="text"
            class="archive-search-input"
            placeholder="Search archive…   (/ to focus, j/k navigate)"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
          <button
            class="archive-sort-btn"
            onClick={() => setSort((s) => (s === 'newest' ? 'oldest' : 'newest'))}
            title={sort() === 'newest' ? 'Newest first' : 'Oldest first'}
            aria-label="Toggle sort order"
          >
            <Show when={sort() === 'newest'} fallback={<ArrowUpNarrowWide size={13} stroke-width={1.75} />}>
              <ArrowDownWideNarrow size={13} stroke-width={1.75} />
            </Show>
          </button>
          <Show when={filterBadgeCount() > 0}>
            <button class="archive-search-clear" onClick={clearFilters} aria-label="Clear all filters">
              clear {filterBadgeCount()}
            </button>
          </Show>
        </div>
        {/* Active filter chips */}
        <Show when={activeTag() || activeSource()}>
          <div class="archive-active-filters">
            <Show when={activeTag()}>
              <span class="archive-filter-chip">
                <Hash size={10} stroke-width={1.75} /> {activeTag()}
                <button onClick={() => setActiveTag(null)} aria-label="Remove tag filter">×</button>
              </span>
            </Show>
            <Show when={activeSource()}>
              <span class="archive-filter-chip">
                <FileText size={10} stroke-width={1.75} /> {sourceName(activeSource()!)}
                <button onClick={() => setActiveSource(null)} aria-label="Remove source filter">×</button>
              </span>
            </Show>
          </div>
        </Show>
        <div class="archive-list-body">
          <Show when={loading()}>
            <div class="archive-empty">Loading…</div>
          </Show>
          <Show when={!loading() && visibleItems().length === 0}>
            <div class="archive-empty">
              <Show when={query() || activeTag() || activeSource()} fallback="No entries">
                No matches · <button class="archive-inline-action" onClick={clearFilters}>clear filters</button>
              </Show>
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
                      <span class="archive-item-file">· {sourceName(item.file_path)}</span>
                    </Show>
                    <Show when={item.confidence}>
                      <span class="archive-item-conf">· {item.confidence}</span>
                    </Show>
                  </div>
                  <div class="archive-item-excerpt">{excerpt(item.body)}</div>
                  <Show when={(item.tags || []).length > 0}>
                    <div class="archive-item-tags">
                      <For each={(item.tags || []).slice(0, 4)}>
                        {(t) => <span class="archive-item-tag">#{t}</span>}
                      </For>
                    </div>
                  </Show>
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
                    <Show when={item().confidence}>
                      <span class="archive-detail-conf">· confidence {item().confidence}</span>
                    </Show>
                  </div>
                  <h2 class="archive-detail-title">{item().name}</h2>
                  <div class="archive-detail-meta">
                    <Show when={item().timestamp}>
                      <span><Clock size={11} stroke-width={1.75} /> {formatTimestamp(item().timestamp)}</span>
                    </Show>
                    <Show when={item().file_path}>
                      <span>· source: <button class="archive-source-btn" onClick={() => setActiveSource(item().file_path!)}>
                        {sourceName(item().file_path)}
                      </button></span>
                    </Show>
                    <Show when={item().source_url}>
                      <span>· <a href={item().source_url} target="_blank" rel="noopener noreferrer">external ↗</a></span>
                    </Show>
                    <div class="archive-detail-actions">
                      <button class="archive-show-in-graph" onClick={() => showInGraph(item())} title="Show in graph (Enter)">
                        <ExternalLink size={11} stroke-width={1.75} /> Show in graph
                      </button>
                      <button class="archive-show-in-graph" onClick={() => copyAsMarkdown(item())} title="Copy as markdown (y)">
                        <Show when={copied()} fallback={<><Copy size={11} stroke-width={1.75} /> Copy</>}>
                          <><Check size={11} stroke-width={1.75} /> Copied</>
                        </Show>
                      </button>
                    </div>
                  </div>
                  <Show when={(item().tags || []).length > 0}>
                    <div class="archive-detail-tags">
                      <For each={item().tags || []}>
                        {(t) => (
                          <button
                            class={`archive-tag-chip ${activeTag() === t ? 'active' : ''}`}
                            onClick={() => setActiveTag(activeTag() === t ? null : t)}
                          >
                            <Hash size={10} stroke-width={1.75} />
                            <span>{t}</span>
                          </button>
                        )}
                      </For>
                    </div>
                  </Show>
                </header>
                <div
                  class="archive-detail-body markdown-body"
                  innerHTML={renderMarkdown(item().body)}
                />
                <Show when={relatedItems().length > 0}>
                  <section class="archive-related">
                    <h3 class="archive-related-title">
                      <FileText size={13} stroke-width={1.75} />
                      From same session ({sourceName(item().file_path)})
                    </h3>
                    <div class="archive-related-list">
                      <For each={relatedItems()}>
                        {(rel) => {
                          const rcat = categoryMeta(rel.category);
                          const RIcon = rcat?.Icon;
                          return (
                            <button class="archive-related-item" onClick={() => setSelected(rel)}>
                              <Show when={RIcon}>
                                <Dynamic component={RIcon!} size={11} stroke-width={1.75} style={{ color: rcat!.color }} />
                              </Show>
                              <span class="archive-related-name">{rel.name}</span>
                              <Show when={rel.timestamp}>
                                <span class="archive-related-time">{formatTimestamp(rel.timestamp)}</span>
                              </Show>
                            </button>
                          );
                        }}
                      </For>
                    </div>
                  </section>
                </Show>
              </>
            );
          }}
        </Show>
      </article>
    </div>
  );
}
