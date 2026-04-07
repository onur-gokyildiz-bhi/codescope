import { setShowShortcuts } from '../store';

const SHORTCUTS: [string, string][] = [
  ['Ctrl+K', 'Command palette'],
  ['F', 'Toggle file tree'],
  ['C', 'Toggle conversations'],
  ['?', 'Show shortcuts'],
  ['1', 'Graph view'],
  ['2', 'Circle pack view'],
  ['3', 'Hotspot view'],
  ['Esc', 'Close overlays'],
];

export default function ShortcutHelp() {
  function close() {
    setShowShortcuts(false);
  }

  return (
    <div class="shortcuts-overlay">
      <div class="palette-backdrop" onClick={close} />
      <div class="glass" style="min-width:320px">
        <div class="panel-header">
          <span>Keyboard Shortcuts</span>
          <button class="panel-close" onClick={close}>&times;</button>
        </div>
        <div class="shortcuts-grid">
          {SHORTCUTS.map(([key, desc]) => (
            <>
              <span class="shortcut-key">{key}</span>
              <span class="shortcut-desc">{desc}</span>
            </>
          ))}
        </div>
      </div>
    </div>
  );
}
