import { createSignal, createEffect, Show, For } from 'solid-js';
import { Tabs } from '@kobalte/core/tabs';
import { selectedNode, setSelectedNode, setCenterNode, setCurrentFile, setErrorMsg } from '../store';
import { api } from '../api';
import { X } from 'lucide-solid';

type Tab = 'details' | 'callers' | 'callees' | 'knowledge';

function isKnowledge(kind: string): boolean {
  return kind?.startsWith('knowledge:') ?? false;
}

function kindAccent(kind: string): string {
  if (kind === 'function') return 'var(--accent)';
  if (kind === 'class')    return 'var(--accent-magenta)';
  if (kind === 'file')     return 'var(--accent-lime)';
  if (kind === 'module')   return 'var(--accent-violet)';
  if (kind === 'cluster')  return 'var(--accent-violet)';
  if (isKnowledge(kind))   return 'var(--accent-violet)';
  return 'var(--accent-amber)';
}

export default function NodeInspector() {
  const [tab, setTab] = createSignal<Tab>('details');
  const [detail, setDetail] = createSignal<any>(null);
  const [knowDetail, setKnowDetail] = createSignal<any>(null);
  const [convData, setConvData] = createSignal<any[]>([]);
  const [loadingDetail, setLoadingDetail] = createSignal(false);

  createEffect(async () => {
    const node = selectedNode();
    setDetail(null);
    setKnowDetail(null);
    setConvData([]);
    setTab('details');
    if (!node?.name) return;

    setLoadingDetail(true);
    try {
      if (isKnowledge(node.kind)) {
        const d = await api.knowledgeDetail(node.id);
        setKnowDetail(d);
      } else {
        const [d, convs] = await Promise.allSettled([
          api.nodeDetail(node.name),
          api.conversations(),
        ]);
        if (d.status === 'fulfilled') setDetail(d.value);
        if (convs.status === 'fulfilled') {
          const list: any[] = convs.value?.sessions || convs.value || [];
          setConvData(list.slice(0, 30));
        }
      }
    } catch (e) {
      setErrorMsg(`Inspector load failed: ${String(e)}`);
    } finally {
      setLoadingDetail(false);
    }
  });

  function navigate(name: string) {
    setCenterNode(name);
  }

  const node = selectedNode;

  return (
    <>
      {/* Header with gradient border */}
      <div class="right-panel-header" style={{ position: 'relative' }}>
        <div style={{ flex: 1, 'min-width': 0 }}>
          <div class="right-panel-title">{node()?.name ?? 'Details'}</div>
          <Show when={node()?.file_path}>
            <div style={{ 'font-size': 'var(--font-xs)', color: 'var(--text-dim)', 'font-family': 'var(--font-mono)', 'margin-top': '2px', overflow: 'hidden', 'text-overflow': 'ellipsis', 'white-space': 'nowrap' }}>
              {node()!.file_path}{node()!.line ? `:${node()!.line}` : ''}
            </div>
          </Show>
        </div>
        <Show when={node()?.kind}>
          <span
            class="right-panel-kind"
            style={{ color: kindAccent(node()!.kind), 'border-color': `${kindAccent(node()!.kind)}40`, background: `${kindAccent(node()!.kind)}10` }}
          >
            {isKnowledge(node()!.kind) ? node()!.kind.split(':')[1] : node()!.kind}
          </span>
        </Show>
        <button
          class="panel-close"
          style={{ 'margin-left': '8px', 'flex-shrink': 0 }}
          onClick={() => setSelectedNode(null)}
          aria-label="Close inspector"
        >
          <X size={14} stroke-width={1.75} />
        </button>
      </div>

      {/* Kobalte Tabs */}
      <Tabs
        value={tab()}
        onChange={(v) => setTab(v as Tab)}
        class="inspector-tabs-root"
      >
        <Tabs.List class="inspector-tabs" style={{ 'overflow-x': 'auto', 'scroll-snap-type': 'x mandatory', 'scrollbar-width': 'none' }}>
          <Tabs.Trigger value="details" class="inspector-tab">Details</Tabs.Trigger>
          <Show when={!isKnowledge(node()?.kind ?? '')}>
            <Tabs.Trigger value="callers" class="inspector-tab">
              Callers{detail()?.callers?.length ? ` (${detail().callers.length})` : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="callees" class="inspector-tab">
              Callees{detail()?.callees?.length ? ` (${detail().callees.length})` : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="knowledge" class="inspector-tab">Knowledge</Tabs.Trigger>
          </Show>
          <Tabs.Indicator class="inspector-tab-indicator" />
        </Tabs.List>

        {/* Body */}
        <div class="right-panel-body">
          <Show when={loadingDetail()}>
            <div style={{ padding: 'var(--space-4)', display: 'flex', gap: 'var(--space-2)', 'flex-direction': 'column' }}>
              <div class="skeleton" style={{ height: '16px', width: '80%' }} />
              <div class="skeleton" style={{ height: '16px', width: '60%' }} />
              <div class="skeleton" style={{ height: '16px', width: '70%' }} />
            </div>
          </Show>

          {/* Details tab */}
          <Tabs.Content value="details">
            <Show when={!loadingDetail()}>
              <div class="panel-body">
                <Show when={node()?.confidence}>
                  <div class="detail-row">
                    <span class="detail-label">Confidence</span>
                    <span class={`confidence-badge confidence-${node()!.confidence}`}>{node()!.confidence}</span>
                  </div>
                </Show>

                <Show when={node()?.tags?.length}>
                  <div class="detail-row">
                    <span class="detail-label">Tags</span>
                    <span class="detail-value" style={{ 'line-height': '1.8' }}>
                      <For each={node()!.tags}>
                        {(tag: string) => <span class="tag-pill">{tag}</span>}
                      </For>
                    </span>
                  </div>
                </Show>

                <Show when={node()?.content}>
                  <div class="detail-section">
                    <div class="detail-section-title">Content</div>
                    <p style={{ 'font-size': 'var(--font-sm)', color: 'var(--text-dim)', 'line-height': '1.6', margin: 0, 'white-space': 'pre-wrap' }}>
                      {node()!.content}
                    </p>
                  </div>
                </Show>

                <Show when={node()?.source_url}>
                  <div class="detail-row" style={{ 'margin-top': 'var(--space-3)' }}>
                    <span class="detail-label">Source</span>
                    <a class="detail-link" href={node()!.source_url} target="_blank" rel="noopener">
                      {node()!.source_url}
                    </a>
                  </div>
                </Show>

                {/* Code entity signature */}
                <Show when={detail()?.signature}>
                  <div class="detail-section">
                    <div class="detail-section-title">Signature</div>
                    <pre style={{ 'font-family': 'var(--font-mono)', 'font-size': 'var(--font-xs)', 'white-space': 'pre-wrap', color: 'var(--accent)', background: 'rgba(0,229,255,0.04)', padding: 'var(--space-2) var(--space-3)', 'border-radius': '6px', border: '1px solid rgba(0,229,255,0.12)' }}>
                      {detail().signature}
                    </pre>
                  </div>
                </Show>

                {/* Open file button */}
                <Show when={node()?.file_path}>
                  <div style={{ 'margin-top': 'var(--space-4)' }}>
                    <button
                      style={{ padding: '5px 12px', background: 'rgba(0,229,255,0.08)', border: '1px solid rgba(0,229,255,0.25)', 'border-radius': '6px', color: 'var(--accent)', 'font-size': 'var(--font-xs)', cursor: 'pointer', transition: 'all var(--transition)', 'font-family': 'var(--font-body)' }}
                      onClick={() => setCurrentFile(node()!.file_path)}
                    >
                      Open file
                    </button>
                  </div>
                </Show>

                {/* Knowledge detail entity content */}
                <Show when={knowDetail()?.entity?.[0]?.content}>
                  <div class="detail-section">
                    <div class="detail-section-title">Full Content</div>
                    <p style={{ 'font-size': 'var(--font-sm)', color: 'var(--text-dim)', 'line-height': '1.6', margin: 0, 'white-space': 'pre-wrap' }}>
                      {knowDetail().entity[0].content}
                    </p>
                  </div>
                </Show>

                <Show when={knowDetail()?.supports?.length}>
                  <div class="detail-section">
                    <div class="detail-section-title" style={{ color: 'var(--accent-lime)' }}>
                      Supports ({knowDetail().supports.length})
                    </div>
                    <For each={knowDetail().supports}>
                      {(s: any) => (
                        <div class="knowledge-link-item">
                          <span class="detail-link" onClick={() => setSelectedNode({ id: s.id, name: s.title, kind: `knowledge:${s.kind || 'concept'}` })}>
                            {s.title}
                          </span>
                          <Show when={s.context}>
                            <span class="knowledge-link-context">{s.context}</span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>

                <Show when={knowDetail()?.contradicts?.length}>
                  <div class="detail-section">
                    <div class="detail-section-title" style={{ color: 'var(--accent-magenta)' }}>
                      Contradicts ({knowDetail().contradicts.length})
                    </div>
                    <For each={knowDetail().contradicts}>
                      {(s: any) => (
                        <div class="knowledge-link-item">
                          <span class="detail-link" style={{ color: 'var(--accent-magenta)' }} onClick={() => setSelectedNode({ id: s.id, name: s.title, kind: `knowledge:${s.kind || 'concept'}` })}>
                            {s.title}
                          </span>
                          <Show when={s.context}>
                            <span class="knowledge-link-context">{s.context}</span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>

                <Show when={(knowDetail()?.related_out?.length || 0) + (knowDetail()?.related_in?.length || 0) > 0}>
                  <div class="detail-section">
                    <div class="detail-section-title">
                      Related ({(knowDetail()?.related_out?.length || 0) + (knowDetail()?.related_in?.length || 0)})
                    </div>
                    <For each={[...(knowDetail()?.related_out || []), ...(knowDetail()?.related_in || [])]}>
                      {(s: any) => (
                        <div class="knowledge-link-item">
                          <span class="detail-link" onClick={() => setSelectedNode({ id: s.id, name: s.title, kind: `knowledge:${s.kind || 'concept'}` })}>
                            {s.title}
                          </span>
                          <Show when={s.relation}>
                            <span class="knowledge-link-context">{s.relation}</span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>
          </Tabs.Content>

          {/* Callers tab */}
          <Tabs.Content value="callers">
            <Show when={!loadingDetail()}>
              <div class="panel-body">
                <Show when={detail()?.callers?.length} fallback={<div style={{ color: 'var(--text-dim)', 'font-size': 'var(--font-sm)' }}>No callers found.</div>}>
                  <For each={detail().callers}>
                    {(c: any) => (
                      <div class="detail-link" onClick={() => navigate(c.name || c)}>
                        <span style={{ 'font-family': 'var(--font-mono)', 'font-size': 'var(--font-xs)' }}>{c.name || c}</span>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </Show>
          </Tabs.Content>

          {/* Callees tab */}
          <Tabs.Content value="callees">
            <Show when={!loadingDetail()}>
              <div class="panel-body">
                <Show when={detail()?.callees?.length} fallback={<div style={{ color: 'var(--text-dim)', 'font-size': 'var(--font-sm)' }}>No callees found.</div>}>
                  <For each={detail().callees}>
                    {(c: any) => (
                      <div class="detail-link" onClick={() => navigate(c.name || c)}>
                        <span style={{ 'font-family': 'var(--font-mono)', 'font-size': 'var(--font-xs)' }}>{c.name || c}</span>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </Show>
          </Tabs.Content>

          {/* Knowledge tab */}
          <Tabs.Content value="knowledge">
            <Show when={!loadingDetail()}>
              <div class="panel-body">
                <Show when={convData().length > 0} fallback={<div style={{ color: 'var(--text-dim)', 'font-size': 'var(--font-sm)' }}>No conversations recorded for this codebase.</div>}>
                  <div style={{ 'font-size': 'var(--font-xs)', color: 'var(--text-dim)', 'margin-bottom': 'var(--space-2)' }}>
                    {convData().length} recent sessions
                  </div>
                  <For each={convData().slice(0, 20)}>
                    {(c: any) => (
                      <div class="conv-item">
                        <div class="conv-item-title">{c.title || c.id || 'Session'}</div>
                        <Show when={c.summary || c.content}>
                          <div class="conv-item-body">{(c.summary || c.content || '').slice(0, 100)}…</div>
                        </Show>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </Show>
          </Tabs.Content>
        </div>
      </Tabs>
    </>
  );
}
