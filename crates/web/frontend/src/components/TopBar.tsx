import { Show, For } from 'solid-js';
import {
  viewMode, setViewMode, showFiles, setShowFiles,
  showConv, setShowConv, setShowPalette, setShowShortcuts,
  stats, projectVersion,
} from '../store';
import ProjectSwitcher from './ProjectSwitcher';

type View = 'graph' | 'pack' | 'hotspot' | 'cluster';

const VIEW_LABELS: { id: View; label: string; key: string }[] = [
  { id: 'graph',   label: 'Graph',    key: '1' },
  { id: 'pack',    label: 'Pack',     key: '2' },
  { id: 'hotspot', label: 'Hotspot',  key: '3' },
  { id: 'cluster', label: 'Clusters', key: '4' },
];

export default function TopBar() {
  return (
    <header class="header">
      {/* Left: logo */}
      <span class="header-logo">Codescope</span>

      {/* Project switcher */}
      <ProjectSwitcher />

      {/* Center: search trigger */}
      <input
        class="header-search"
        type="text"
        placeholder="Search… (Ctrl+K)"
        onFocus={() => setShowPalette(true)}
        readOnly
        style={{ cursor: 'pointer' }}
      />

      {/* View mode tabs */}
      <div class="header-views">
        <For each={VIEW_LABELS}>
          {(v) => (
            <button
              class={`view-btn ${viewMode() === v.id ? 'active' : ''}`}
              onClick={() => setViewMode(v.id)}
              title={`${v.label} (${v.key})`}
            >
              {v.label}
            </button>
          )}
        </For>
      </div>

      {/* Stats pills */}
      <Show when={stats()}>
        <div class="stats-badge">
          <span class="stat-pill">
            <span class="val">{(stats()!.functions ?? 0).toLocaleString()}</span> fn
          </span>
          <span class="stat-pill">
            <span class="val">{(stats()!.files ?? 0).toLocaleString()}</span> files
          </span>
          <Show when={(stats()!.knowledge ?? 0) > 0}>
            <span class="stat-pill" style={{ color: 'var(--accent-violet)' }}>
              <span class="val">{stats()!.knowledge!.toLocaleString()}</span> knowledge
            </span>
          </Show>
        </div>
      </Show>
      <Show when={!stats() && projectVersion() >= 0}>
        {/* skeleton pills while loading */}
        <div class="stats-badge" style={{ gap: '4px' }}>
          <span class="skeleton" style={{ width: '56px', height: '20px' }} />
          <span class="skeleton" style={{ width: '48px', height: '20px' }} />
        </div>
      </Show>

      {/* Toggles */}
      <button
        class={`header-btn ${showFiles() ? 'active' : ''}`}
        onClick={() => setShowFiles(v => !v)}
        title="File tree (F)"
      >
        Files
      </button>
      <button
        class={`header-btn ${showConv() ? 'active' : ''}`}
        onClick={() => setShowConv(v => !v)}
        title="Conversations (C)"
      >
        Conv
      </button>
      <button
        class="settings-btn"
        onClick={() => setShowShortcuts(v => !v)}
        title="Keyboard shortcuts (?)"
      >
        ?
      </button>
    </header>
  );
}
