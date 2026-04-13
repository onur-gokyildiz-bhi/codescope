export const KIND_COLORS: Record<string, string> = {
  function: '#58A6FF',
  class: '#3FB950',
  struct: '#3FB950',
  trait: '#3FB950',
  interface: '#3FB950',
  file: '#8B949E',
  caller: '#F85149',
  callee: '#D29922',
  sibling: '#BC8CFF',
  skill: '#DA77F2',
  config: '#E6B450',
  doc: '#79C0FF',
  package: '#FF7B72',
  'knowledge:concept': '#F0883E',
  'knowledge:entity': '#DA77F2',
  'knowledge:source': '#79C0FF',
  'knowledge:claim': '#D29922',
  'knowledge:decision': '#F85149',
  'knowledge:pattern': '#3FB950',
};

export const EDGE_COLORS: Record<string, string> = {
  calls: '#30363D',
  same_file: '#1C2333',
  contains: '#21262D',
  wikilink: '#BC8CFF',
  supports: '#3FB950',
  contradicts: '#F85149',
  related_to: '#DA77F2',
};

export function kindColor(node: any): string {
  return KIND_COLORS[node.kind] || '#8B949E';
}

export function edgeColor(link: any): string {
  return EDGE_COLORS[link.kind] || '#30363D';
}

function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = ((hash << 5) - hash + str.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

export function moduleColor(filePath: string): string {
  const parts = filePath.split('/');
  const module = parts.slice(0, Math.min(2, parts.length - 1)).join('/');
  const h = hashString(module) % 360;
  return `hsl(${h}, 65%, 55%)`;
}

export const DIM_COLOR = '#1c2333';
export const DIM_EDGE = '#0d1117';
export const HIGHLIGHT_EDGE = '#58a6ff';
