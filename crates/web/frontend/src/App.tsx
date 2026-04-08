import { onMount, Show, createEffect } from 'solid-js';
import {
  viewMode, setViewMode, showFiles, setShowFiles,
  showConv, setShowConv,
  showPalette, setShowPalette, showShortcuts, setShowShortcuts,
  currentFile, stats, setStats, selectedNode,
  projectVersion,
} from './store';
import { api } from './api';
import { registerShortcut } from './utils/keyboard';
import Graph3D from './components/Graph3D';
import CirclePack from './components/CirclePack';
import HotspotChart from './components/HotspotChart';
import CommandPalette from './components/CommandPalette';
import Sidebar from './components/Sidebar';
import FileTree from './components/FileTree';
import SourceViewer from './components/SourceViewer';
import ShortcutHelp from './components/ShortcutHelp';
import ConvPanel from './components/ConvPanel';
import DepthSlider from './components/DepthSlider';
import Legend from './components/Legend';
import Minimap from './components/Minimap';
import ProjectSwitcher from './components/ProjectSwitcher';

type View = 'graph' | 'pack' | 'hotspot' | 'skill' | 'cluster';

const VIEW_LABELS: { id: View; label: string }[] = [
  { id: 'graph', label: 'Graph' },
  { id: 'pack', label: 'Pack' },
  { id: 'hotspot', label: 'Hotspot' },
  { id: 'skill', label: 'Skills' },
  { id: 'cluster', label: 'Clusters' },
];

export default function App() {
  onMount(() => {
    registerShortcut('cmd+k', () => setShowPalette(v => !v));
    registerShortcut('f', () => setShowFiles(v => !v));
    registerShortcut('c', () => setShowConv(v => !v));
    registerShortcut('?', () => setShowShortcuts(v => !v));
    registerShortcut('1', () => setViewMode('graph'));
    registerShortcut('2', () => setViewMode('pack'));
    registerShortcut('3', () => setViewMode('hotspot'));
    registerShortcut('escape', () => {
      setShowPalette(false);
      setShowShortcuts(false);
    });
  });

  // Re-fetch stats on project switch
  createEffect(async () => {
    projectVersion(); // track project changes
    try {
      const s = await api.stats();
      setStats(s);
    } catch { /* server may not be ready */ }
  });

  return (
    <div class="app-layout">
      <header class="header">
        <span class="header-logo">Codescope</span>
        <ProjectSwitcher />
        <input
          class="header-search"
          type="text"
          placeholder="Search... (Ctrl+K)"
          onFocus={() => setShowPalette(true)}
          readOnly
        />
        <div class="header-views">
          {VIEW_LABELS.map(v => (
            <button
              class={`view-btn ${viewMode() === v.id ? 'active' : ''}`}
              onClick={() => setViewMode(v.id)}
            >
              {v.label}
            </button>
          ))}
        </div>
        <Show when={stats()}>
          <span class="stats-badge">
            {stats().functions ?? 0} fn | {stats().files ?? 0} files
          </span>
        </Show>
        <button class="settings-btn" onClick={() => setShowShortcuts(v => !v)} title="Shortcuts">
          ?
        </button>
      </header>

      <div class="main-area">
        <Show when={viewMode() === 'graph'}>
          <Graph3D />
        </Show>
        <Show when={viewMode() === 'pack'}>
          <CirclePack />
        </Show>
        <Show when={viewMode() === 'hotspot'}>
          <HotspotChart />
        </Show>

        <Show when={viewMode() === 'graph'}>
          <div class="depth-control glass" style="position:absolute;bottom:8px;left:50%;transform:translateX(-50%);z-index:40">
            <DepthSlider />
          </div>
          <Legend />
          <Minimap />
        </Show>

        <Show when={showFiles()}>
          <div class="float-panel left panel">
            <FileTree />
          </div>
        </Show>

        <Show when={selectedNode()}>
          <div class="float-panel right panel">
            <Sidebar />
          </div>
        </Show>

        <Show when={showConv()}>
          <div class="float-panel right panel" style="top:auto;bottom:8px;max-height:300px">
            <ConvPanel />
          </div>
        </Show>
      </div>

      <Show when={showPalette()}>
        <CommandPalette />
      </Show>

      <Show when={showShortcuts()}>
        <ShortcutHelp />
      </Show>

      <Show when={currentFile()}>
        <SourceViewer />
      </Show>
    </div>
  );
}
