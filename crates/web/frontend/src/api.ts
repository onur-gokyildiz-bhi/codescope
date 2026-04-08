import { currentProject } from './store';

const BASE = '';

function withRepo(url: string): string {
  const p = currentProject();
  if (!p) return url;
  const sep = url.includes('?') ? '&' : '?';
  return `${url}${sep}repo=${encodeURIComponent(p)}`;
}

export const api = {
  projects: () =>
    fetch(`${BASE}/api/projects`)
      .then(r => r.json())
      .catch(() => ({ projects: [], active: [] })),

  stats: () => fetch(withRepo(`${BASE}/api/stats`)).then(r => r.json()),

  graph: (center?: string, depth = 2) => {
    const p = new URLSearchParams();
    if (center) p.set('center', center);
    p.set('depth', String(depth));
    const proj = currentProject();
    if (proj) p.set('repo', proj);
    return fetch(`${BASE}/api/graph?${p}`).then(r => r.json());
  },

  search: (q: string) =>
    fetch(withRepo(`${BASE}/api/search?q=${encodeURIComponent(q)}`)).then(r => r.json()),

  nodeDetail: (name: string) =>
    fetch(withRepo(`${BASE}/api/node-detail?name=${encodeURIComponent(name)}`)).then(r => r.json()),

  fileContent: (path: string) =>
    fetch(withRepo(`${BASE}/api/file-content?path=${encodeURIComponent(path)}`)).then(r => r.json()),

  files: () => fetch(withRepo(`${BASE}/api/files`)).then(r => r.json()),

  conversations: () => fetch(withRepo(`${BASE}/api/conversations`)).then(r => r.json()),

  hotspots: () => fetch(withRepo(`${BASE}/api/hotspots`)).then(r => r.json()),

  clusters: () => fetch(withRepo(`${BASE}/api/clusters`)).then(r => r.json()),

  skillGraph: () => fetch(withRepo(`${BASE}/api/skill-graph`)).then(r => r.json()),

  rawQuery: (q: string) =>
    fetch(withRepo(`${BASE}/api/query?q=${encodeURIComponent(q)}`)).then(r => r.json()),
};
