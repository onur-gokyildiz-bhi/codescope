import { Show, For, onMount, createSignal } from 'solid-js';
import {
  currentProject, setCurrentProject,
  availableProjects, setAvailableProjects,
  setProjectVersion, setStats, setGraphData, setSelectedNode,
  setCenterNode, setErrorMsg,
} from '../store';
import { api } from '../api';

export default function ProjectSwitcher() {
  const [loaded, setLoaded] = createSignal(false);

  onMount(async () => {
    try {
      const data = await api.projects();
      const projects: string[] = data.projects || [];
      const active: string[] = data.active || [];
      setAvailableProjects(projects);
      if (!currentProject() && projects.length > 0) {
        // Prefer the currently-active repo (the one the server was launched
        // with) over the first alphabetical project — otherwise the graph
        // loads the wrong repo on first paint.
        const preferred = active.find((r) => projects.includes(r)) ?? projects[0];
        setCurrentProject(preferred);
      }
      setLoaded(true);
    } catch { /* intentionally ignored: not in daemon mode (stdio) — switcher stays hidden */ }
  });

  const switchProject = (name: string) => {
    setCurrentProject(name);
    // Reset UI state for clean switch
    setSelectedNode(null);
    setCenterNode(null);
    setGraphData({ nodes: [], links: [] });
    setStats(null);
    // Bump version to trigger re-fetch in all components
    setProjectVersion(v => v + 1);
    // Re-fetch stats immediately
    api.stats().then(s => setStats(s)).catch((e) => setErrorMsg(`Failed to refresh stats: ${String(e)}`));
  };

  return (
    <Show when={loaded() && availableProjects().length > 1}>
      <select
        class="project-switcher"
        value={currentProject() || ''}
        onChange={(e) => switchProject(e.currentTarget.value)}
      >
        <For each={availableProjects()}>
          {(name) => <option value={name}>{name}</option>}
        </For>
      </select>
    </Show>
  );
}
