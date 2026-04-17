import { onMount, onCleanup, createEffect, createSignal, Show } from 'solid-js';
import {
  graphData, setGraphData, selectedNode, setSelectedNode,
  hoveredNode, setHoveredNode, graphDepth,
  centerNode, setCenterNode, colorMode,
  repelStrength, linkDistance, setCurrentFile,
  projectVersion, loading, setLoading, setErrorMsg,
  kindFilter,
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
  // Knowledge/conversation entity types (non-prefixed, from _global DB)
  decision:  '#a371f7',   // violet
  problem:   '#ff3df5',   // magenta
  solution:  '#7cff5c',   // lime green
  topic:     '#ffb347',   // amber
  knowledge: '#00e5ff',   // teal/cyan
  conversation: '#64748b',
};

const KNOWLEDGE_COLORS: Record<string, string> = {
  concept:    '#ffb347',
  entity:     '#a371f7',
  source:     '#00e5ff',
  claim:      '#ffd080',
  decision:   '#a371f7',   // violet — decision
  pattern:    '#7cff5c',
  problem:    '#ff3df5',   // magenta — problem
  solution:   '#7cff5c',   // green — solution
  correction: '#00e5ff',
  topic:      '#ffb347',   // amber — topic
  knowledge:  '#00e5ff',   // teal — knowledge
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
  // Direct kind (from _global DB without knowledge: prefix)
  return KIND_COLORS[n.kind] || '#64748b';
}

function edgeColor(l: any): string {
  const k = l.kind;
  if (k === 'calls')        return 'rgba(0,229,255,0.6)';
  if (k === 'supports')     return 'rgba(124,255,92,0.7)';
  if (k === 'contradicts')  return 'rgba(255,61,245,0.7)';
  if (k === 'related_to')   return 'rgba(163,113,247,0.6)';
  if (k === 'imports')      return 'rgba(255,179,71,0.5)';
  if (k === 'contains')     return 'rgba(100,116,139,0.4)';
  if (k === 'discussed_in') return 'rgba(100,116,139,0.4)';
  if (k === 'decided_about')return 'rgba(163,113,247,0.5)';
  if (k === 'solves_for')   return 'rgba(124,255,92,0.5)';
  if (k === 'co_discusses') return 'rgba(255,179,71,0.4)';
  if (k === 'links_to')     return 'rgba(100,116,139,0.4)';
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
    isKnowledge(n.kind || '') ? 5 :
    n.kind === 'decision' ? 8 :
    n.kind === 'problem' ? 8 :
    n.kind === 'solution' ? 8 : 1;
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
  let THREE_ref: any;
  // Separate overlay mesh for selected-node glow ring (built once, repositioned each tick)
  let glowOverlay: any = null;
  let glowMesh: any = null;
  let rafHandle = 0;
  const [totalNodes, setTotalNodes] = createSignal(0);

  function disposeGlow() {
    if (glowMesh) {
      glowMesh.geometry?.dispose();
      glowMesh.material?.dispose();
      glowOverlay?.remove(glowMesh);
      glowMesh = null;
    }
    if (glowOverlay && graph) {
      try { graph.scene()?.remove(glowOverlay); } catch { /* ignore */ }
    }
    glowOverlay = null;
  }

  function buildGlow(node: any, THREE: any, getColor: (n: any) => string) {
    disposeGlow();
    if (!node || !graph) return;

    const scene = graph.scene?.();
    if (!scene) return;

    const baseSz = node.kind === 'file' ? 3 : node.kind === 'class' ? 2.5 : 2;
    const group = new THREE.Group();

    // Core sphere
    const sphereGeo = new THREE.SphereGeometry(baseSz, 16, 12);
    const sphereMat = new THREE.MeshLambertMaterial({ color: getColor(node) });
    group.add(new THREE.Mesh(sphereGeo, sphereMat));

    // Glow ring
    const ringGeo = new THREE.RingGeometry(baseSz + 1.5, baseSz + 2.5, 32);
    const ringMat = new THREE.MeshBasicMaterial({
      color: getColor(node),
      transparent: true,
      opacity: 0.6,
      side: THREE.DoubleSide,
    });
    glowMesh = new THREE.Mesh(ringGeo, ringMat);
    group.add(glowMesh);

    glowOverlay = group;
    scene.add(glowOverlay);
  }

  function syncGlowPosition() {
    if (!glowOverlay || !graph) return;
    const sel = selectedNode();
    if (!sel) { glowOverlay.visible = false; return; }

    // Find node position from the force graph
    const data = graphData();
    const node = data.nodes.find((n: any) => n.id === sel.id);
    if (node && typeof node.x === 'number') {
      glowOverlay.position.set(node.x, node.y ?? 0, node.z ?? 0);
      glowOverlay.visible = true;
    } else {
      glowOverlay.visible = false;
    }
  }

  onMount(async () => {
    const [ForceGraph3D, THREE] = await Promise.all([
      import('3d-force-graph').then(m => m.default),
      import('three'),
    ]);
    THREE_ref = THREE;

    graph = ForceGraph3D()(container)
      .backgroundColor('#07080c')
      .nodeVal((n: any) => {
        if (n.kind === 'cluster') return 14;
        if (isKnowledge(n.kind || '')) return 4;
        if (n.kind === 'decision' || n.kind === 'problem' || n.kind === 'solution') return 3;
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
          // Decision/Problem/Solution nodes from _global DB: octahedron
          if (n.kind === 'decision' || n.kind === 'problem' || n.kind === 'solution' || n.kind === 'topic' || n.kind === 'knowledge') {
            const geo = new THREE.OctahedronGeometry(4);
            const mat = new THREE.MeshLambertMaterial({
              color: getNodeColor(n),
              transparent: true,
              opacity: 0.9,
            });
            return new THREE.Mesh(geo, mat);
          }
          // Selected node: handled via glowOverlay, return default sphere
          return undefined;
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
        if (k === 'decided_about' || k === 'solves_for') return 1.5;
        if (k === 'calls') return 0.8;
        return 0;
      })
      // NOTE: 3d-force-graph 1.80 has no .linkLineDash — removed. Knowledge
      // vs code edges stay distinguishable via linkColor + linkWidth alone.
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
        // Rebuild glow for newly selected node
        buildGlow(node, THREE, getNodeColor);
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

      // Wire bloom into engine tick — with feature check
      if (typeof graph.onEngineTick === 'function') {
        graph.onEngineTick(() => {
          syncGlowPosition();
          composer.render();
          return false;
        });
      } else {
        // Fallback: RAF loop for bloom + glow sync when onEngineTick is unavailable
        const controls = graph.controls?.();
        if (controls) {
          controls.addEventListener('change', () => {
            syncGlowPosition();
            composer.render();
          });
        }
        function rafLoop() {
          syncGlowPosition();
          composer.render();
          rafHandle = requestAnimationFrame(rafLoop);
        }
        rafHandle = requestAnimationFrame(rafLoop);
      }
    } catch {
      // bloom unavailable — use RAF just for glow sync
      function glowLoop() {
        syncGlowPosition();
        rafHandle = requestAnimationFrame(glowLoop);
      }
      rafHandle = requestAnimationFrame(glowLoop);
    }
  });

  createEffect(async () => {
    projectVersion();
    const center = centerNode();
    const kf = kindFilter();
    setLoading(true);
    try {
      const data = await api.graph(center || undefined, graphDepth());
      let filtered = { ...data };
      if (kf) {
        const isKnowledgeFilter = kf === 'knowledge';
        filtered.nodes = (data.nodes || []).filter((n: any) => {
          if (isKnowledgeFilter) return isKnowledge(n.kind || '');
          return n.kind === kf;
        });
        const keepIds = new Set(filtered.nodes.map((n: any) => n.id));
        filtered.links = (data.links || []).filter((l: any) => {
          const sid = typeof l.source === 'object' ? l.source.id : l.source;
          const tid = typeof l.target === 'object' ? l.target.id : l.target;
          return keepIds.has(sid) && keepIds.has(tid);
        });
      }
      const capped = applyLOD(filtered);
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

  // Rebuild glow when selectedNode changes externally
  createEffect(() => {
    const sel = selectedNode();
    if (!sel) {
      disposeGlow();
    } else if (THREE_ref && graph) {
      buildGlow(sel, THREE_ref, getNodeColor);
    }
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
    cancelAnimationFrame(rafHandle);
    disposeGlow();
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
