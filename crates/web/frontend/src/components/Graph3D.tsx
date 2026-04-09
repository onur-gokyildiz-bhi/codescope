import { onMount, onCleanup, createEffect } from 'solid-js';
import {
  graphData, setGraphData, selectedNode, setSelectedNode,
  hoveredNode, setHoveredNode, graphDepth,
  centerNode, setCenterNode, colorMode,
  repelStrength, linkDistance, setCurrentFile,
  projectVersion,
} from '../store';
import { api } from '../api';
import { kindColor, moduleColor, edgeColor, DIM_COLOR, DIM_EDGE, HIGHLIGHT_EDGE } from '../utils/colors';

export default function Graph3D() {
  let container!: HTMLDivElement;
  let graph: any;

  onMount(async () => {
    const ForceGraph3D = (await import('3d-force-graph')).default;
    graph = ForceGraph3D()(container)
      .backgroundColor('#0d1117')
      .nodeVal((n: any) => {
        const s = n.kind === 'file' ? 2 : n.kind === 'class' ? 1.5 : 1;
        return s * s;
      })
      .nodeColor((n: any) => {
        const hov = hoveredNode();
        if (hov) {
          const connected = getConnected(hov);
          return connected.has(n.id) ? getNodeColor(n) : DIM_COLOR;
        }
        return getNodeColor(n);
      })
      .nodeLabel((n: any) => `${n.kind}: ${n.name}`)
      .linkColor((l: any) => {
        const hov = hoveredNode();
        if (hov) {
          const connected = getConnected(hov);
          const sid = typeof l.source === 'object' ? l.source.id : l.source;
          const tid = typeof l.target === 'object' ? l.target.id : l.target;
          return connected.has(sid) && connected.has(tid) ? HIGHLIGHT_EDGE : DIM_EDGE;
        }
        return edgeColor(l);
      })
      .linkOpacity(0.4)
      .linkDirectionalParticles((l: any) => l.kind === 'calls' ? 2 : 0)
      .linkDirectionalParticleSpeed(0.005)
      .onNodeHover((node: any) => setHoveredNode(node))
      .onNodeClick((node: any) => {
        setSelectedNode(node);
        setCenterNode(node.name);
      })
      .onNodeRightClick((node: any) => {
        if (node.file_path) setCurrentFile(node.file_path);
      })
      .d3AlphaDecay(0.02)
      .d3VelocityDecay(0.3)
      .warmupTicks(80)
      .cooldownTicks(120);

  });

  // Load graph data on mount and on project switch
  createEffect(async () => {
    projectVersion(); // re-fetch on project switch
    const center = centerNode();
    try {
      const data = await api.graph(center || undefined, graphDepth());
      setGraphData(data);
    } catch { /* graph load may fail if no data indexed */ }
  });

  function getNodeColor(n: any): string {
    return colorMode() === 'module' && n.file_path
      ? moduleColor(n.file_path)
      : kindColor(n);
  }

  function getConnected(node: any): Set<string> {
    const set = new Set<string>();
    set.add(node.id);
    const data = graphData();
    data.links.forEach((l: any) => {
      const sid = typeof l.source === 'object' ? l.source.id : l.source;
      const tid = typeof l.target === 'object' ? l.target.id : l.target;
      if (sid === node.id) set.add(tid);
      if (tid === node.id) set.add(sid);
    });
    return set;
  }

  createEffect(() => {
    const data = graphData();
    if (graph) graph.graphData({ nodes: data.nodes || [], links: data.links || [] });
  });

  createEffect(() => {
    if (!graph) return;
    graph.d3Force('charge')?.strength(repelStrength());
    graph.d3Force('link')?.distance(linkDistance());
    graph.d3ReheatSimulation();
  });

  createEffect(async () => {
    // depth change also triggers re-fetch
    const center = centerNode();
    const depth = graphDepth();
    if (center) {
      try {
        const data = await api.graph(center, depth);
        setGraphData(data);
      } catch { /* ignore fetch errors */ }
    }
  });

  onCleanup(() => {
    if (graph) graph._destructor?.();
  });

  return <div ref={container} style={{ width: '100%', height: '100%' }} />;
}
