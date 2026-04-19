import { currentProject } from './store';

const BASE = '';

function withRepo(url: string): string {
  const p = currentProject();
  if (!p) return url;
  const sep = url.includes('?') ? '&' : '?';
  return `${url}${sep}repo=${encodeURIComponent(p)}`;
}

async function fetchJson<T>(url: string): Promise<T> {
  const r = await fetch(url);
  if (!r.ok) {
    const body = await r.text();
    const msg = body.trim() || `HTTP ${r.status}`;
    throw new Error(msg);
  }
  const body = await r.text();
  try {
    return JSON.parse(body) as T;
  } catch {
    throw new Error(body.slice(0, 200) || 'Invalid JSON from server');
  }
}

export const api = {
  projects: () =>
    fetchJson<{ projects: string[]; active: string[] }>(`${BASE}/api/projects`)
      .catch(() => ({ projects: [], active: [] })),

  stats: () => fetchJson(withRepo(`${BASE}/api/stats`)),

  graph: (center?: string, depth = 2, clusterMode = 'auto') => {
    const p = new URLSearchParams();
    if (center) p.set('center', center);
    p.set('depth', String(depth));
    p.set('cluster_mode', clusterMode);
    const proj = currentProject();
    if (proj) p.set('repo', proj);
    return fetchJson(`${BASE}/api/graph?${p}`);
  },

  search: (q: string) =>
    fetchJson(withRepo(`${BASE}/api/search?q=${encodeURIComponent(q)}`)),

  nodeDetail: (name: string) =>
    fetchJson(withRepo(`${BASE}/api/node-detail?name=${encodeURIComponent(name)}`)),

  fileContent: (path: string) =>
    fetchJson(withRepo(`${BASE}/api/file-content?path=${encodeURIComponent(path)}`)),

  files: () => fetchJson(withRepo(`${BASE}/api/files`)),

  conversations: () => fetchJson(withRepo(`${BASE}/api/conversations`)),

  hotspots: () => fetchJson(withRepo(`${BASE}/api/hotspots`)),

  clusters: () => fetchJson(withRepo(`${BASE}/api/clusters`)),

  skillGraph: () => fetchJson(withRepo(`${BASE}/api/skill-graph`)),

  knowledgeDetail: (id: string) =>
    fetchJson(withRepo(`${BASE}/api/knowledge-detail?id=${encodeURIComponent(id)}`)),

  rawQuery: (q: string) =>
    fetchJson(withRepo(`${BASE}/api/query?q=${encodeURIComponent(q)}`)),

  // Phase 3 Dream — narrated tours through the knowledge graph.
  dreamArcs: () =>
    fetchJson<{ arcs: DreamArcSummary[] }>(withRepo(`${BASE}/api/dream/arcs`)),

  dreamArc: (id: string) =>
    fetchJson<DreamArcDetail>(withRepo(`${BASE}/api/dream/arc/${encodeURIComponent(id)}`)),
};

export type DreamArcSummary = {
  id: string;
  title: string;
  tag: string;
  count: number;
  first_at: string | null;
  last_at: string | null;
  kinds: string[];
};

export type DreamScene = {
  id: string;
  kind: string;
  title: string;
  content: string;
  created_at: string | null;
  tags: string[];
  narration: string;
};

export type DreamArcDetail = {
  id: string;
  title: string;
  tag: string;
  scenes: DreamScene[];
};
