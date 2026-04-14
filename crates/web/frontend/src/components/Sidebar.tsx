import { createSignal, createEffect, Show, For } from 'solid-js';
import { selectedNode, setSelectedNode, setCenterNode, setShowSidebar, setErrorMsg } from '../store';
import { api } from '../api';

function isKnowledge(kind: string): boolean {
  return kind?.startsWith('knowledge:') ?? false;
}

export default function Sidebar() {
  const [detail, setDetail] = createSignal<any>(null);
  const [knowDetail, setKnowDetail] = createSignal<any>(null);

  createEffect(async () => {
    const node = selectedNode();
    setDetail(null);
    setKnowDetail(null);
    if (!node?.name) return;

    if (isKnowledge(node.kind)) {
      try {
        const d = await api.knowledgeDetail(node.id);
        setKnowDetail(d);
      } catch (e) { setErrorMsg(`Failed to load knowledge detail: ${String(e)}`); }
    } else {
      try {
        const d = await api.nodeDetail(node.name);
        setDetail(d);
      } catch (e) { setErrorMsg(`Failed to load node detail: ${String(e)}`); }
    }
  });

  function close() {
    setSelectedNode(null);
    setShowSidebar(false);
  }

  function navigate(name: string) {
    setCenterNode(name);
  }

  return (
    <>
      <div class="panel-header">
        <span>{selectedNode()?.name ?? 'Details'}</span>
        <button class="panel-close" onClick={close}>&times;</button>
      </div>
      <div class="panel-body">
        <Show when={selectedNode()}>
          <div class="detail-row">
            <span class="detail-label">Kind</span>
            <span class="detail-value">
              <Show when={isKnowledge(selectedNode().kind)} fallback={selectedNode().kind}>
                <span class="knowledge-badge" data-kind={selectedNode().kind.split(':')[1]}>
                  {selectedNode().kind.split(':')[1]}
                </span>
              </Show>
            </span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Name</span>
            <span class="detail-value mono">{selectedNode().name}</span>
          </div>
          <Show when={selectedNode().file_path}>
            <div class="detail-row">
              <span class="detail-label">File</span>
              <span class="detail-value mono">
                {selectedNode().file_path}
                {selectedNode().line ? `:${selectedNode().line}` : ''}
              </span>
            </div>
          </Show>
          <Show when={selectedNode().confidence}>
            <div class="detail-row">
              <span class="detail-label">Confidence</span>
              <span class={`confidence-badge confidence-${selectedNode().confidence}`}>
                {selectedNode().confidence}
              </span>
            </div>
          </Show>
          <Show when={selectedNode().tags?.length}>
            <div class="detail-row">
              <span class="detail-label">Tags</span>
              <span class="detail-value">
                <For each={selectedNode().tags}>
                  {(tag: string) => <span class="tag-pill">{tag}</span>}
                </For>
              </span>
            </div>
          </Show>
          <Show when={selectedNode().content}>
            <div class="detail-section">
              <div class="detail-section-title">Content</div>
              <p style="font-size:12px;color:var(--text-dim);line-height:1.5;margin:0">
                {selectedNode().content}
              </p>
            </div>
          </Show>
          <Show when={selectedNode().source_url}>
            <div class="detail-row">
              <span class="detail-label">Source</span>
              <a
                class="detail-link"
                href={selectedNode().source_url}
                target="_blank"
                rel="noopener"
              >
                {selectedNode().source_url}
              </a>
            </div>
          </Show>
        </Show>

        {/* Code entity details */}
        <Show when={detail()}>
          <Show when={detail().signature}>
            <div class="detail-section">
              <div class="detail-section-title">Signature</div>
              <pre class="mono" style="font-size:12px;white-space:pre-wrap;color:var(--text-dim)">
                {detail().signature}
              </pre>
            </div>
          </Show>

          <Show when={detail().callers?.length}>
            <div class="detail-section">
              <div class="detail-section-title">Callers ({detail().callers.length})</div>
              <For each={detail().callers}>
                {(c: any) => (
                  <div class="detail-link" onClick={() => navigate(c.name || c)}>
                    {c.name || c}
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={detail().callees?.length}>
            <div class="detail-section">
              <div class="detail-section-title">Callees ({detail().callees.length})</div>
              <For each={detail().callees}>
                {(c: any) => (
                  <div class="detail-link" onClick={() => navigate(c.name || c)}>
                    {c.name || c}
                  </div>
                )}
              </For>
            </div>
          </Show>
        </Show>

        {/* Knowledge entity details */}
        <Show when={knowDetail()}>
          <Show when={knowDetail().entity?.[0]?.content}>
            <div class="detail-section">
              <div class="detail-section-title">Full Content</div>
              <p style="font-size:12px;color:var(--text-dim);line-height:1.5;margin:0;white-space:pre-wrap">
                {knowDetail().entity[0].content}
              </p>
            </div>
          </Show>

          <Show when={knowDetail().supports?.length}>
            <div class="detail-section">
              <div class="detail-section-title">
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

          <Show when={knowDetail().contradicts?.length}>
            <div class="detail-section">
              <div class="detail-section-title" style="color:#F85149">
                Contradicts ({knowDetail().contradicts.length})
              </div>
              <For each={knowDetail().contradicts}>
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

          <Show when={(knowDetail().related_out?.length || 0) + (knowDetail().related_in?.length || 0) > 0}>
            <div class="detail-section">
              <div class="detail-section-title">
                Related ({(knowDetail().related_out?.length || 0) + (knowDetail().related_in?.length || 0)})
              </div>
              <For each={[...(knowDetail().related_out || []), ...(knowDetail().related_in || [])]}>
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
        </Show>
      </div>
    </>
  );
}
