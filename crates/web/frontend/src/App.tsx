import { onMount, Show, createEffect } from 'solid-js';
import {
  viewMode, showFiles,
  showConv, setShowConv,
  setShowPalette, setShowShortcuts, showPalette, showShortcuts,
  setViewMode, currentFile, setStats,
  loading, errorMsg, setErrorMsg,
  projectVersion,
} from './store';
import { api } from './api';
import { registerShortcut } from './utils/keyboard';

import TopBar from './components/TopBar';
import Graph3D from './components/Graph3D';
import CirclePack from './components/CirclePack';
import HotspotChart from './components/HotspotChart';
import ClusterView from './components/ClusterView';
import CommandPalette from './components/CommandPalette';
import FileTree from './components/FileTree';
import SourceViewer from './components/SourceViewer';
import ShortcutHelp from './components/ShortcutHelp';
import ConvPanel from './components/ConvPanel';
import DepthSlider from './components/DepthSlider';
import Legend from './components/Legend';
import Minimap from './components/Minimap';
import RightPanel from './components/RightPanel';
import DreamPlaceholder from './components/DreamPlaceholder';

export default function App() {
  const isDream = typeof window !== 'undefined' && window.location.pathname === '/dream';

  onMount(() => {
    registerShortcut('cmd+k', () => setShowPalette(v => !v));
    // 'f' shortcut is handled in TopBar via store signal directly
    registerShortcut('c', () => setShowConv(v => !v));
    registerShortcut('?', () => setShowShortcuts(v => !v));
    registerShortcut('1', () => setViewMode('graph'));
    registerShortcut('2', () => setViewMode('pack'));
    registerShortcut('3', () => setViewMode('hotspot'));
    registerShortcut('4', () => setViewMode('cluster'));
    registerShortcut('escape', () => {
      setShowPalette(false);
      setShowShortcuts(false);
    });
  });

  createEffect(async () => {
    projectVersion();
    try {
      const s = await api.stats();
      setStats(s);
    } catch { /* stats poll — server may not be ready */ }
  });

  if (isDream) {
    return (
      <div class="app-layout">
        <TopBar />
        <div class="main-area">
          <DreamPlaceholder />
        </div>
      </div>
    );
  }

  return (
    <div class="app-layout">
      <Show when={loading()}>
        <div class="loading-bar" />
      </Show>

      <TopBar />

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
        <Show when={viewMode() === 'cluster'}>
          <ClusterView />
        </Show>

        <Show when={viewMode() === 'graph'}>
          <div class="depth-control glass" style={{ position: 'absolute', bottom: '8px', left: '50%', transform: 'translateX(-50%)', 'z-index': '40' }}>
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

        <RightPanel />

        <Show when={showConv()}>
          <div class="float-panel right panel" style={{ top: 'auto', bottom: '8px', 'max-height': '300px' }}>
            <ConvPanel />
          </div>
        </Show>

        <Show when={errorMsg()}>
          <div class="error-toast">
            <span>{errorMsg()}</span>
            <button class="error-toast-close" onClick={() => setErrorMsg(null)}>&times;</button>
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
