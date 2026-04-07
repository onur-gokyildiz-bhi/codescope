import { createSignal, createEffect, Show, For } from 'solid-js';
import { selectedNode, setSelectedNode, setCenterNode, setShowSidebar } from '../store';
import { api } from '../api';

export default function Sidebar() {
  const [detail, setDetail] = createSignal<any>(null);

  createEffect(async () => {
    const node = selectedNode();
    if (node?.name) {
      try {
        const d = await api.nodeDetail(node.name);
        setDetail(d);
      } catch {
        setDetail(null);
      }
    } else {
      setDetail(null);
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
            <span class="detail-value">{selectedNode().kind}</span>
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
        </Show>

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
      </div>
    </>
  );
}
