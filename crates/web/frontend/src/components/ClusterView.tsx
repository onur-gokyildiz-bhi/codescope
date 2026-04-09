import { createSignal, createEffect, For, Show } from 'solid-js';
import { setCenterNode, projectVersion } from '../store';
import { api } from '../api';

interface Cluster {
  file_path: string;
  fn_count: number;
  functions: string[];
}

export default function ClusterView() {
  const [clusters, setClusters] = createSignal<Cluster[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [expanded, setExpanded] = createSignal<string | null>(null);

  createEffect(async () => {
    projectVersion();
    setLoading(true);
    try {
      const data = await api.clusters();
      const items: Cluster[] = (data || []).map((c: any) => ({
        file_path: c.file_path || '',
        fn_count: c.fn_count || 0,
        functions: c.functions || [],
      }));
      setClusters(items);
    } catch { /* ignore */ }
    setLoading(false);
  });

  const maxCount = () => {
    const c = clusters();
    return c.length ? c[0].fn_count : 1;
  };

  const shortPath = (fp: string) => {
    const parts = fp.replace(/\\/g, '/').split('/');
    return parts.length > 3 ? '.../' + parts.slice(-3).join('/') : fp;
  };

  return (
    <div style="padding:16px;overflow-y:auto;height:100%">
      <h3 style="margin:0 0 12px;font-size:14px;color:var(--text)">File Clusters by Function Count</h3>
      <Show when={loading()}>
        <span style="color:var(--text-dim);font-size:12px">Loading...</span>
      </Show>
      <Show when={!loading() && clusters().length === 0}>
        <span style="color:var(--text-dim);font-size:12px">No cluster data</span>
      </Show>
      <For each={clusters()}>
        {(cluster) => (
          <div style="margin-bottom:6px">
            <div
              style="display:flex;align-items:center;gap:8px;cursor:pointer"
              onClick={() => setExpanded(e => e === cluster.file_path ? null : cluster.file_path)}
            >
              <div style={`height:18px;background:var(--accent);border-radius:3px;min-width:4px;width:${Math.max((cluster.fn_count / maxCount()) * 100, 4)}%`} />
              <span style="font-size:11px;color:var(--text-dim);white-space:nowrap;min-width:28px">{cluster.fn_count}</span>
              <span class="mono" style="font-size:11px;color:var(--text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={cluster.file_path}>
                {shortPath(cluster.file_path)}
              </span>
            </div>
            <Show when={expanded() === cluster.file_path}>
              <div style="padding:4px 0 4px 20px">
                <For each={cluster.functions}>
                  {(fn) => (
                    <div
                      class="mono"
                      style="font-size:10px;color:var(--accent);cursor:pointer;padding:1px 0"
                      onClick={() => setCenterNode(fn)}
                    >
                      {fn}
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}
