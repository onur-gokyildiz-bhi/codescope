const BASE = '';

export const api = {
  stats: () => fetch(`${BASE}/api/stats`).then(r => r.json()),

  graph: (center?: string, depth = 2) => {
    const p = new URLSearchParams();
    if (center) p.set('center', center);
    p.set('depth', String(depth));
    return fetch(`${BASE}/api/graph?${p}`).then(r => r.json());
  },

  search: (q: string) =>
    fetch(`${BASE}/api/search?q=${encodeURIComponent(q)}`).then(r => r.json()),

  nodeDetail: (name: string) =>
    fetch(`${BASE}/api/node-detail?name=${encodeURIComponent(name)}`).then(r => r.json()),

  fileContent: (path: string) =>
    fetch(`${BASE}/api/file-content?path=${encodeURIComponent(path)}`).then(r => r.json()),

  files: () => fetch(`${BASE}/api/files`).then(r => r.json()),

  conversations: () => fetch(`${BASE}/api/conversations`).then(r => r.json()),

  hotspots: () => fetch(`${BASE}/api/hotspots`).then(r => r.json()),

  clusters: () => fetch(`${BASE}/api/clusters`).then(r => r.json()),

  skillGraph: () => fetch(`${BASE}/api/skill-graph`).then(r => r.json()),

  rawQuery: (q: string) =>
    fetch(`${BASE}/api/query?q=${encodeURIComponent(q)}`).then(r => r.json()),
};
