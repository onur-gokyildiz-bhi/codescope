type ShortcutHandler = () => void;

const shortcuts: Map<string, ShortcutHandler> = new Map();

export function registerShortcut(key: string, handler: ShortcutHandler) {
  shortcuts.set(key.toLowerCase(), handler);
}

export function initKeyboard() {
  document.addEventListener('keydown', (e) => {
    // Don't capture when typing in input
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA') {
      if (e.key === 'Escape') {
        (e.target as HTMLElement).blur();
      }
      return;
    }

    // Cmd/Ctrl+K → command palette
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      shortcuts.get('cmd+k')?.();
      return;
    }

    const handler = shortcuts.get(e.key.toLowerCase());
    if (handler) {
      e.preventDefault();
      handler();
    }
  });
}
