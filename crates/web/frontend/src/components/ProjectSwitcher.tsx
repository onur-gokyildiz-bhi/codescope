import { Show, For, onMount, createSignal } from 'solid-js';
import {
  currentProject, setCurrentProject,
  availableProjects, setAvailableProjects,
  setProjectVersion, setStats, setGraphData, setSelectedNode,
  setCenterNode,
} from '../store';
import { api } from '../api';

export default function ProjectSwitcher() {
  const [loaded, setLoaded] = createSignal(false);

  onMount(async () => {
    try {
      const data = await api.projects();
      const projects: string[] = data.projects || [];
      setAvailableProjects(projects);
      if (!currentProject() && projects.length > 0) {
        setCurrentProject(projects[0]);
      }
      setLoaded(true);
    } catch {
      // Not in daemon mode (stdio) — hide the switcher
    }
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
    api.stats().then(s => setStats(s)).catch(() => {});
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
