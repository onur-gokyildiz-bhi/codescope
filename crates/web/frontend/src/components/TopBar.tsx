import { Show, For } from 'solid-js';
import {
  viewMode, setViewMode, showFiles, setShowFiles,
  showConv, setShowConv, setShowPalette, setShowShortcuts,
  stats, projectVersion, setKindFilter,
} from '../store';
import ProjectSwitcher from './ProjectSwitcher';
import {
  FolderTree, MessageSquare, Keyboard, Network,
  CirclePlus, Flame, LayoutGrid, Search, Menu, Archive,
} from 'lucide-solid';

type View = 'graph' | 'pack' | 'hotspot' | 'cluster' | 'archive';

const VIEW_META: { id: View; label: string; key: string; Icon: any }[] = [
  { id: 'graph',   label: 'Graph',    key: '1', Icon: Network    },
  { id: 'pack',    label: 'Pack',     key: '2', Icon: CirclePlus },
  { id: 'hotspot', label: 'Hotspot',  key: '3', Icon: Flame      },
  { id: 'cluster', label: 'Clusters', key: '4', Icon: LayoutGrid },
  { id: 'archive', label: 'Archive',  key: '5', Icon: Archive    },
];

export default function TopBar() {
  return (
    <header class="header">
      {/* Left: logo */}
      <span class="header-logo">Codescope</span>

      {/* Project switcher */}
      <ProjectSwitcher />

      {/* Center: search trigger */}
      <button
        class="header-search-btn"
        onClick={() => setShowPalette(true)}
        title="Search (Ctrl+K)"
        aria-label="Open search"
      >
        <Search size={14} stroke-width={1.75} style={{ 'flex-shrink': 0 }} />
        <span class="header-search-placeholder">Search… (Ctrl+K)</span>
      </button>

      {/* View mode tabs */}
      <div class="header-views">
        <For each={VIEW_META}>
          {(v) => (
            <button
              class={`view-btn ${viewMode() === v.id ? 'active' : ''}`}
              onClick={() => setViewMode(v.id)}
              title={`${v.label} (${v.key})`}
            >
              <v.Icon size={13} stroke-width={1.75} />
              <span class="view-btn-label">{v.label}</span>
            </button>
          )}
        </For>
      </div>

      {/* Stats pills — clickable to filter */}
      <Show when={stats()}>
        <div class="stats-badge">
          <button class="stat-pill" title="Filter: functions" onClick={() => setKindFilter('function')}>
            <span class="val">{(stats()!.functions ?? 0).toLocaleString()}</span> fn
          </button>
          <button class="stat-pill" title="Filter: files" onClick={() => setKindFilter('file')}>
            <span class="val">{(stats()!.files ?? 0).toLocaleString()}</span> files
          </button>
          <Show when={(stats()!.knowledge ?? 0) > 0}>
            <button class="stat-pill" style={{ color: 'var(--accent-violet)' }} title="Filter: knowledge" onClick={() => setKindFilter('knowledge')}>
              <span class="val">{stats()!.knowledge!.toLocaleString()}</span> knowledge
            </button>
          </Show>
        </div>
      </Show>
      <Show when={!stats() && projectVersion() >= 0}>
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
        aria-label="Toggle file tree"
      >
        <FolderTree size={14} stroke-width={1.75} />
        <span class="header-btn-label">Files</span>
      </button>
      <button
        class={`header-btn ${showConv() ? 'active' : ''}`}
        onClick={() => setShowConv(v => !v)}
        title="Conversations (C)"
        aria-label="Toggle conversations"
      >
        <MessageSquare size={14} stroke-width={1.75} />
        <span class="header-btn-label">Conv</span>
      </button>
      <button
        class="settings-btn"
        onClick={() => setShowShortcuts(v => !v)}
        title="Keyboard shortcuts (?)"
        aria-label="Keyboard shortcuts"
      >
        <Keyboard size={14} stroke-width={1.75} />
      </button>

      {/* Mobile menu trigger — hidden on desktop */}
      <button
        class="mobile-menu-btn"
        onClick={() => setShowFiles(v => !v)}
        title="Menu"
        aria-label="Open menu"
      >
        <Menu size={16} stroke-width={1.75} />
      </button>
    </header>
  );
}
