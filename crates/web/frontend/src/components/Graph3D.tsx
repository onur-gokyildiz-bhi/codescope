import { onMount, onCleanup, createEffect, createSignal, Show } from 'solid-js';
import {
  graphData, setGraphData, selectedNode, setSelectedNode,
  hoveredNode, setHoveredNode, graphDepth,
  centerNode, setCenterNode, colorMode,
  repelStrength, linkDistance, setCurrentFile,
  projectVersion, loading, setLoading, setErrorMsg,
} from '../store';
import { api } from '../api';
import { kindColor, moduleColor, edgeColor, DIM_COLOR, DIM_EDGE, HIGHLIGHT_EDGE } from '../utils/colors';

const MAX_VISIBLE_NODES = 500;

function isKnowledge(kind: string): boolean {
  return kind.startsWith('knowledge:');
}

// Rank nodes for LOD: clusters first, then by caller_count (if present), then by kind priority.
function importanceScore(n: any): number {
  if (n.kind === 'cluster') return 1e9;
  const callers = typeof n.caller_count === 'number' ? n.caller_count : 0;
  const kindWeight =
    n.kind === 'file' ? 100 :
    n.kind === 'class' ? 50 :
    n.kind === 'function' ? 10 :
    isKnowledge(n.kind || '') ? 5 : 1;
  return callers * 1000 + kindWeight;
}

function applyLOD(data: { nodes: any[]; links: any[] }): { nodes: any[]; links: any[]; total: number } {
  const nodes = data.nodes || [];
  const links = data.links || [];
  const total = nodes.length;
  if (total <= MAX_VISIBLE_NODES) {
    return { nodes, links, total };
  }
  const sorted = [...nodes].sort((a, b) => importanceScore(b) - importanceScore(a));
  const kept = sorted.slice(0, MAX_VISIBLE_NODES);
  const keepIds = new Set(kept.map(n => n.id));
  const filteredLinks = links.filter((l: any) => {
    const sid = typeof l.source === 'object' ? l.source.id : l.source;
    const tid = typeof l.target === 'object' ? l.target.id : l.target;
    return keepIds.has(sid) && keepIds.has(tid);
  });
  return { nodes: kept, links: filteredLinks, total };
}

export default function Graph3D() {
  let container!: HTMLDivElement;
  let graph: any;
  const [totalNodes, setTotalNodes] = createSignal(0);

  onMount(async () => {
    // Load ForceGraph3D + three together so THREE is guaranteed before any node render.
    const [ForceGraph3D, THREE] = await Promise.all([
      import('3d-force-graph').then(m => m.default),
      import('three'),
    ]);
    graph = ForceGraph3D()(container)
      .backgroundColor('#0d1117')
      .nodeVal((n: any) => {
        if (n.kind === 'cluster') return 12;
        if (isKnowledge(n.kind)) return 4;
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
      .nodeLabel((n: any) => {
        if (isKnowledge(n.kind)) {
          const sub = n.kind.split(':')[1] || 'knowledge';
          return `${sub}: ${n.name}${n.confidence ? ` (${n.confidence})` : ''}`;
        }
        return `${n.kind}: ${n.name}`;
      })
      .nodeThreeObject((n: any) => {
        if (!isKnowledge(n.kind)) return undefined;
        try {
          const geo = new THREE.OctahedronGeometry(4);
          const mat = new THREE.MeshLambertMaterial({
            color: getNodeColor(n),
            transparent: true,
            opacity: 0.85,
          });
          return new THREE.Mesh(geo, mat);
        } catch (e) {
          setErrorMsg(String(e));
          return undefined;
        }
      })
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
      .linkWidth((l: any) => {
        const k = l.kind;
        if (k === 'supports' || k === 'contradicts' || k === 'related_to') return 1.5;
        return 0;
      })
      .linkLineDash((l: any) => {
        const k = l.kind;
        if (k === 'supports' || k === 'contradicts' || k === 'related_to') return [4, 2];
        return null;
      })
      .linkDirectionalParticles((l: any) => {
        if (l.kind === 'calls') return 2;
        if (l.kind === 'supports') return 1;
        if (l.kind === 'contradicts') return 1;
        return 0;
      })
      .linkDirectionalParticleSpeed(0.005)
      .linkDirectionalParticleColor((l: any) => edgeColor(l))
      .onNodeHover((node: any) => setHoveredNode(node))
      .onNodeClick((node: any) => {
        setSelectedNode(node);
        if (!isKnowledge(node.kind)) {
          setCenterNode(node.name);
        }
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
    projectVersion();
    const center = centerNode();
    setLoading(true);
    try {
      const data = await api.graph(center || undefined, graphDepth());
      const capped = applyLOD(data);
      setTotalNodes(capped.total);
      setGraphData({ nodes: capped.nodes, links: capped.links });
      setErrorMsg(null);
    } catch (e: any) {
      setErrorMsg(`Failed to load graph data: ${String(e?.message || e)}`);
    } finally {
      setLoading(false);
    }
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
    const center = centerNode();
    const depth = graphDepth();
    if (center) {
      setLoading(true);
      try {
        const data = await api.graph(center, depth);
        const capped = applyLOD(data);
        setTotalNodes(capped.total);
        setGraphData({ nodes: capped.nodes, links: capped.links });
        setErrorMsg(null);
      } catch (e: any) {
        setErrorMsg(`Failed to load graph data: ${String(e?.message || e)}`);
      } finally {
        setLoading(false);
      }
    }
  });

  onCleanup(() => {
    if (graph) graph._destructor?.();
  });

  return (
    <div style={{ width: '100%', height: '100%', position: 'relative' }}>
      <div ref={container} style={{ width: '100%', height: '100%' }} />
      <Show when={graphData().nodes.length === 0 && !loading()}>
        <div class="empty-state">
          <h2>No graph yet</h2>
          <p>Index your codebase to see the graph:</p>
          <pre>cd your-project &amp;&amp; codescope index .</pre>
          <p>Then reload this page.</p>
        </div>
      </Show>
      <Show when={totalNodes() > MAX_VISIBLE_NODES}>
        <div class="lod-indicator">
          Showing {MAX_VISIBLE_NODES} of {totalNodes()} nodes. Use cluster mode or search to see more.
        </div>
      </Show>
    </div>
  );
}
