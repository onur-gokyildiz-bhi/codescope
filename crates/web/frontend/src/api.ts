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

  insight: () => fetchJson<InsightResponse>(`${BASE}/api/insight`),

  dreamSuggestTags: () =>
    fetchJson<{ suggestions: DreamSuggestion[] }>(withRepo(`${BASE}/api/dream/suggest-tags`)),

  dreamPatterns: () =>
    fetchJson<{ patterns: DreamPattern[] }>(`${BASE}/api/dream/patterns`),

  dreamApplyTag: async (id: string, tag: string): Promise<{ ok: boolean }> => {
    const proj = currentProject();
    const url = `${BASE}/api/dream/apply-tag${proj ? `?repo=${encodeURIComponent(proj)}` : ''}`;
    const r = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, tag }),
    });
    if (!r.ok) {
      throw new Error(await r.text());
    }
    return r.json();
  },
};

export type DreamSuggestionCandidate = {
  tag: string;
  score: number;
  matched_words: string[];
};

export type DreamSuggestion = {
  id: string;
  title: string;
  kind: string;
  candidates: DreamSuggestionCandidate[];
};

export type DreamPatternRepo = {
  repo: string;
  count: number;
  example_title: string;
};

export type DreamPattern = {
  tag: string;
  title: string;
  repos: DreamPatternRepo[];
  total: number;
};

export type InsightResponse = {
  summary: {
    total_calls: number;
    repos: Record<string, number>;
    hours: Record<string, number>;
    first_ts: number | null;
    last_ts: number | null;
  };
  gain: {
    total_calls: number;
    tokens_per_call_est: number;
    tokens_saved_est: number;
    first_used: string | null;
    last_used: string | null;
  };
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
  duplicate_of?: {
    id: string;
    index: number;
    score: number;
  };
};

export type DreamArcDetail = {
  id: string;
  title: string;
  tag: string;
  scenes: DreamScene[];
};
