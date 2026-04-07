import { createSignal, onMount, For, Show } from 'solid-js';
import { api } from '../api';
import { setCenterNode } from '../store';

interface Hotspot {
  name: string;
  kind: string;
  score: number;
  lines?: number;
}

export default function HotspotChart() {
  const [hotspots, setHotspots] = createSignal<Hotspot[]>([]);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const data = await api.hotspots();
      const items: Hotspot[] = (data || [])
        .map((h: any) => ({
          name: h.name || h.id,
          kind: h.kind || 'function',
          score: h.score || h.lines || h.complexity || 0,
          lines: h.lines,
        }))
        .sort((a: Hotspot, b: Hotspot) => b.score - a.score)
        .slice(0, 30);
      setHotspots(items);
    } catch { /* ignore */ }
    setLoading(false);
  });

  function maxScore(): number {
    const h = hotspots();
    return h.length ? h[0].score : 1;
  }

  return (
    <div style="padding:24px;overflow-y:auto;height:100%">
      <h3 style="font-size:16px;margin-bottom:16px;color:var(--text)">Hotspots</h3>
      <Show when={loading()}>
        <span style="color:var(--text-dim)">Loading...</span>
      </Show>
      <Show when={!loading() && hotspots().length === 0}>
        <span style="color:var(--text-dim)">No hotspot data available</span>
      </Show>
      <For each={hotspots()}>
        {(h) => (
          <div class="hotspot-bar" onClick={() => setCenterNode(h.name)} style="cursor:pointer">
            <span class="hotspot-name mono" title={h.name}>{h.name}</span>
            <div style="flex:1;position:relative;height:16px;background:var(--bg);border-radius:3px">
              <div
                class="hotspot-fill"
                style={`width:${(h.score / maxScore()) * 100}%`}
              />
            </div>
            <span class="hotspot-value">{h.score}</span>
          </div>
        )}
      </For>
    </div>
  );
}
