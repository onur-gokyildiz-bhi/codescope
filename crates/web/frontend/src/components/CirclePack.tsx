import { onMount, onCleanup } from 'solid-js';
import { setCenterNode } from '../store';
import { api } from '../api';
import { KIND_COLORS } from '../utils/colors';

export default function CirclePack() {
  let container!: HTMLDivElement;
  let cleanup: (() => void) | undefined;

  onMount(async () => {
    const d3 = await import('https://cdn.jsdelivr.net/npm/d3@7/+esm' as any);

    let rawData: any;
    try {
      rawData = await api.graph(undefined, 3);
    } catch {
      return;
    }

    // Build hierarchy: group nodes by file path segments
    const groups: Record<string, any[]> = {};
    for (const node of rawData.nodes || []) {
      const path = node.file_path || 'unknown';
      const dir = path.split('/').slice(0, -1).join('/') || 'root';
      if (!groups[dir]) groups[dir] = [];
      groups[dir].push(node);
    }

    const root = {
      name: 'root',
      children: Object.entries(groups).map(([dir, nodes]) => ({
        name: dir,
        children: nodes.map(n => ({
          name: n.name,
          kind: n.kind,
          value: n.kind === 'file' ? 4 : n.kind === 'class' ? 3 : 1,
          fullNode: n,
        })),
      })),
    };

    const rect = container.getBoundingClientRect();
    const width = rect.width || 800;
    const height = rect.height || 600;

    const svg = d3.create('svg')
      .attr('viewBox', `0 0 ${width} ${height}`)
      .attr('width', '100%')
      .attr('height', '100%');

    const pack = d3.pack()
      .size([width, height])
      .padding(3);

    const hierarchy = d3.hierarchy(root)
      .sum((d: any) => d.value || 0)
      .sort((a: any, b: any) => (b.value || 0) - (a.value || 0));

    const packedRoot = pack(hierarchy);

    const node = svg.selectAll('g')
      .data(packedRoot.descendants())
      .join('g')
      .attr('transform', (d: any) => `translate(${d.x},${d.y})`);

    node.append('circle')
      .attr('r', (d: any) => d.r)
      .attr('fill', (d: any) => {
        if (!d.depth) return 'none';
        if (d.data.kind) return KIND_COLORS[d.data.kind] || '#8b949e';
        return '#161b22';
      })
      .attr('fill-opacity', (d: any) => d.children ? 0.15 : 0.6)
      .attr('stroke', (d: any) => d.children ? '#30363d' : 'none')
      .attr('stroke-width', 0.5)
      .style('cursor', (d: any) => d.data.fullNode ? 'pointer' : 'default')
      .on('click', (_: any, d: any) => {
        if (d.data.fullNode) {
          setCenterNode(d.data.fullNode.name);
        }
      });

    node.filter((d: any) => d.r > 15 && !d.children)
      .append('text')
      .text((d: any) => d.data.name)
      .attr('text-anchor', 'middle')
      .attr('dy', '0.35em')
      .attr('fill', '#e6edf3')
      .attr('font-size', (d: any) => Math.min(d.r / 3, 12))
      .attr('pointer-events', 'none');

    container.appendChild(svg.node()!);

    cleanup = () => {
      svg.remove();
    };
  });

  onCleanup(() => cleanup?.());

  return <div ref={container} class="circle-pack-container" />;
}
