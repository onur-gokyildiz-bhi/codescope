import { onMount, onCleanup, createEffect, createSignal, Show } from 'solid-js';
import {
  graphData, setGraphData, selectedNode, setSelectedNode,
  hoveredNode, setHoveredNode, graphDepth,
  centerNode, setCenterNode, colorMode,
  repelStrength, linkDistance, setCurrentFile,
  projectVersion, loading, setLoading, setErrorMsg,
} from '../store';
import { api } from '../api';
import { moduleColor, DIM_COLOR, DIM_EDGE, HIGHLIGHT_EDGE } from '../utils/colors';

const MAX_VISIBLE_NODES = 500;

// ─── Cyberpunk color palette ──────────────────────────────────────
const KIND_COLORS: Record<string, string> = {
  function:  '#00e5ff',   // neon cyan
  method:    '#00e5ff',
  class:     '#ff3df5',   // magenta
  struct:    '#ff3df5',
  interface: '#ff88fb',
  file:      '#7cff5c',   // lime green
  module:    '#a371f7',   // violet
  cluster:   '#a371f7',
  enum:      '#ffb347',   // amber
  trait:     '#ffb347',
  type:      '#ffd080',
  macro:     '#00e5ff',
  constant:  '#7cff5c',
  variable:  '#64748b',
};

const KNOWLEDGE_COLORS: Record<string, string> = {
  concept:    '#ffb347',
  entity:     '#a371f7',
  source:     '#00e5ff',
  claim:      '#ffd080',
  decision:   '#ff3df5',
  pattern:    '#7cff5c',
  problem:    '#ff6b6b',
  correction: '#00e5ff',
  default:    '#a371f7',
};

function isKnowledge(kind: string): boolean {
  return kind?.startsWith('knowledge:');
}

function kindColor(n: any): string {
  if (isKnowledge(n.kind || '')) {
    const sub = (n.kind || '').split(':')[1] || 'default';
    return KNOWLEDGE_COLORS[sub] || KNOWLEDGE_COLORS.default;
  }
  return KIND_COLORS[n.kind] || '#64748b';
}

function edgeColor(l: any): string {
  const k = l.kind;
  if (k === 'calls')       return 'rgba(0,229,255,0.6)';
  if (k === 'supports')    return 'rgba(124,255,92,0.7)';
  if (k === 'contradicts') return 'rgba(255,61,245,0.7)';
  if (k === 'related_to')  return 'rgba(163,113,247,0.6)';
  if (k === 'imports')     return 'rgba(255,179,71,0.5)';
  if (k === 'contains')    return 'rgba(100,116,139,0.4)';
  return 'rgba(100,116,139,0.3)';
}

// ─── LOD ─────────────────────────────────────────────────────────
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
  if (total <= MAX_VISIBLE_NODES) return { nodes, links, total };

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
    const [ForceGraph3D, THREE] = await Promise.all([
      import('3d-force-graph').then(m => m.default),
      import('three'),
    ]);

    graph = ForceGraph3D()(container)
      .backgroundColor('#07080c')
      .nodeVal((n: any) => {
        if (n.kind === 'cluster') return 14;
        if (isKnowledge(n.kind || '')) return 4;
        const s = n.kind === 'file' ? 2.2 : n.kind === 'class' ? 1.8 : 1;
        return s * s;
      })
      .nodeColor((n: any) => {
        const hov = hoveredNode();
        if (hov) {
          const connected = getConnected(hov);
          return connected.has(n.id) ? getNodeColor(n) : DIM_COLOR;
        }
        const sel = selectedNode();
        if (sel && n.id === sel.id) return '#ffffff';
        return getNodeColor(n);
      })
      .nodeLabel((n: any) => {
        if (isKnowledge(n.kind || '')) {
          const sub = n.kind.split(':')[1] || 'knowledge';
          return `<span style="font-family:Inter,sans-serif;font-size:12px">${sub}: <b>${n.name}</b>${n.confidence ? ` (${n.confidence})` : ''}</span>`;
        }
        return `<span style="font-family:Inter,sans-serif;font-size:12px">${n.kind}: <b>${n.name}</b></span>`;
      })
      .nodeThreeObject((n: any) => {
        try {
          // Knowledge nodes: octahedron
          if (isKnowledge(n.kind || '')) {
            const geo = new THREE.OctahedronGeometry(4);
            const mat = new THREE.MeshLambertMaterial({
              color: getNodeColor(n),
              transparent: true,
              opacity: 0.9,
            });
            return new THREE.Mesh(geo, mat);
          }
          // Cluster nodes: larger icosahedron in violet
          if (n.kind === 'cluster') {
            const geo = new THREE.IcosahedronGeometry(8, 0);
            const mat = new THREE.MeshLambertMaterial({
              color: '#a371f7',
              transparent: true,
              opacity: 0.85,
            });
            return new THREE.Mesh(geo, mat);
          }
          // Selected node: glow ring
          if (selectedNode()?.id === n.id) {
            const group = new THREE.Group();
            const baseSz = n.kind === 'file' ? 3 : n.kind === 'class' ? 2.5 : 2;
            const geo = new THREE.SphereGeometry(baseSz, 12, 8);
            const mat = new THREE.MeshLambertMaterial({ color: getNodeColor(n) });
            group.add(new THREE.Mesh(geo, mat));
            // outer glow ring
            const ringGeo = new THREE.RingGeometry(baseSz + 1.5, baseSz + 2.5, 32);
            const ringMat = new THREE.MeshBasicMaterial({
              color: getNodeColor(n),
              transparent: true,
              opacity: 0.6,
              side: THREE.DoubleSide,
            });
            group.add(new THREE.Mesh(ringGeo, ringMat));
            return group;
          }
          return undefined; // default sphere
        } catch (e) {
          setErrorMsg(String(e));
          return undefined;
        }
      })
      .nodeThreeObjectExtend(false)
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
      .linkOpacity(0.5)
      .linkWidth((l: any) => {
        const k = l.kind;
        if (k === 'supports' || k === 'contradicts' || k === 'related_to') return 1.5;
        if (k === 'calls') return 0.8;
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
      .linkDirectionalParticleWidth(1.5)
      .linkDirectionalParticleColor((l: any) => edgeColor(l))
      .onNodeHover((node: any) => setHoveredNode(node))
      .onNodeClick((node: any) => {
        setSelectedNode(node);
        if (!isKnowledge(node.kind || '')) {
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

    // Post-processing: bloom via postprocessing library
    try {
      const { EffectComposer, RenderPass, EffectPass, BloomEffect, KernelSize } = await import('postprocessing');

      const renderer = graph.renderer();
      const scene = graph.scene();
      const camera = graph.camera();

      const composer = new EffectComposer(renderer);
      composer.addPass(new RenderPass(scene, camera));

      const bloomEffect = new BloomEffect({
        intensity: 1.0,
        luminanceThreshold: 0.6,
        luminanceSmoothing: 0.3,
        kernelSize: KernelSize.MEDIUM,
      });
      composer.addPass(new EffectPass(camera, bloomEffect));

      // Override render loop
      graph.onEngineTick(() => {
        composer.render();
        return false;
      });
    } catch {
      // bloom unavailable — graph still works without it
    }
  });

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
          Showing {MAX_VISIBLE_NODES} of {totalNodes()} nodes — use cluster mode or search to navigate
        </div>
      </Show>
    </div>
  );
}
