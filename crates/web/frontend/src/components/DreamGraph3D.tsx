// DreamGraph3D — Phase 3 iter 3.
//
// Standalone 3D tour-graph for the Dream view. Each arc scene is a
// node; consecutive scenes are linked so the arc renders as a
// glowing path in space. When `currentIndex` changes, the camera
// flies to the active node. No reliance on the main Graph3D store
// (kindFilter, centerNode, etc.) — Dream owns its own 3D scene so
// the main graph state is preserved when the user flips back.
//
// Props
// - scenes: ordered list of DreamScene objects for the active arc.
// - currentIndex: which scene the camera should be centred on.
// - onNodeClick: forwarded so the parent can react to user-driven
//   jumps (e.g. "I clicked scene 4, set index to 4").

import { createEffect, onCleanup, onMount } from 'solid-js';
import type { DreamScene } from '../api';

// Same palette as Graph3D so colours feel continuous across views.
const KIND_COLORS: Record<string, string> = {
  decision: '#a371f7',
  problem: '#ff3df5',
  solution: '#7cff5c',
  concept: '#ffb347',
  claim: '#ffd080',
  correction: '#00e5ff',
  pattern: '#7cff5c',
  topic: '#ffb347',
  knowledge: '#00e5ff',
  default: '#a371f7',
};

function kindColor(kind: string): string {
  return KIND_COLORS[kind] || KIND_COLORS.default;
}

type Props = {
  scenes: DreamScene[];
  currentIndex: number;
  onNodeClick?: (index: number) => void;
};

export default function DreamGraph3D(props: Props) {
  let container!: HTMLDivElement;
  let graph: any = null;
  let rafHandle = 0;

  onMount(async () => {
    const [ForceGraph3D, THREE] = await Promise.all([
      import('3d-force-graph').then((m) => m.default),
      import('three'),
    ]);

    graph = ForceGraph3D()(container)
      .backgroundColor('rgba(7, 8, 12, 0)')
      .nodeVal(4)
      .nodeColor((n: any) =>
        n.__active ? '#ffffff' : kindColor(n.kind || 'default'),
      )
      .nodeLabel((n: any) =>
        `<span style="font-family:Inter,sans-serif;font-size:12px;max-width:320px;display:inline-block">${escape(n.title)}</span>`,
      )
      .nodeThreeObject((n: any) => {
        const baseSize = 3.5;
        const group = new THREE.Group();
        // Core octahedron — same shape as knowledge nodes in the
        // main Graph3D so the visual language is consistent.
        const core = new THREE.OctahedronGeometry(baseSize);
        const coreMat = new THREE.MeshLambertMaterial({
          color: n.__active ? '#ffffff' : kindColor(n.kind || 'default'),
          transparent: true,
          opacity: 0.95,
        });
        group.add(new THREE.Mesh(core, coreMat));
        if (n.__active) {
          // Halo ring for the active scene. Kept thin so bloom
          // pushes it to a glow without washing out the node.
          const ring = new THREE.RingGeometry(baseSize + 1.5, baseSize + 2.2, 48);
          const ringMat = new THREE.MeshBasicMaterial({
            color: kindColor(n.kind || 'default'),
            transparent: true,
            opacity: 0.8,
            side: THREE.DoubleSide,
          });
          group.add(new THREE.Mesh(ring, ringMat));
        }
        return group;
      })
      .nodeThreeObjectExtend(false)
      .linkColor(() => 'rgba(163,113,247,0.4)')
      .linkWidth(1.4)
      .linkDirectionalParticles(3)
      .linkDirectionalParticleSpeed(0.004)
      .linkDirectionalParticleWidth(1.5)
      .linkDirectionalParticleColor(() => '#a371f7')
      .d3AlphaDecay(0.04)
      .d3VelocityDecay(0.35)
      .warmupTicks(60)
      .cooldownTicks(100)
      .onNodeClick((n: any) => {
        if (props.onNodeClick && typeof n.__index === 'number') {
          props.onNodeClick(n.__index);
        }
      });

    // Bloom pass — same approach as Graph3D. Fail-soft: if the
    // postprocessing import blows up, the base render still works.
    try {
      const { EffectComposer, RenderPass, EffectPass, BloomEffect, KernelSize } =
        await import('postprocessing');
      const composer = new EffectComposer(graph.renderer());
      composer.addPass(new RenderPass(graph.scene(), graph.camera()));
      const bloom = new BloomEffect({
        intensity: 1.2,
        luminanceThreshold: 0.5,
        luminanceSmoothing: 0.3,
        kernelSize: KernelSize.MEDIUM,
      });
      composer.addPass(new EffectPass(graph.camera(), bloom));
      if (typeof graph.onEngineTick === 'function') {
        graph.onEngineTick(() => {
          composer.render();
          return false;
        });
      } else {
        const loop = () => {
          composer.render();
          rafHandle = requestAnimationFrame(loop);
        };
        rafHandle = requestAnimationFrame(loop);
      }
    } catch {
      /* bloom unavailable — base render is fine */
    }

    // Initial data load once the graph is live.
    applyData();
    // Delay the camera snap so the force simulation has time to
    // place nodes in space before we fly to one.
    setTimeout(focusActive, 600);
  });

  // Rebuild graph data whenever the arc changes. scenes and
  // currentIndex are both tracked so the active flag re-renders.
  createEffect(() => {
    // Touch both signals so Solid re-runs on either change.
    props.scenes;
    props.currentIndex;
    if (graph) applyData();
  });

  // Camera fly-to on index change — separate effect so data
  // rebuilds don't trigger a redundant animation.
  createEffect(() => {
    props.currentIndex;
    if (graph) focusActive();
  });

  function applyData() {
    const scenes = props.scenes ?? [];
    const active = props.currentIndex;
    const nodes = scenes.map((s, i) => ({
      id: s.id || `scene-${i}`,
      title: s.title,
      kind: s.kind,
      __index: i,
      __active: i === active,
    }));
    // Sequential "storyline" links — scene N → scene N+1.
    const links = [];
    for (let i = 0; i + 1 < nodes.length; i++) {
      links.push({ source: nodes[i].id, target: nodes[i + 1].id });
    }
    graph.graphData({ nodes, links });
  }

  function focusActive() {
    const scenes = props.scenes ?? [];
    const idx = props.currentIndex;
    if (idx < 0 || idx >= scenes.length) return;
    const target = scenes[idx];
    if (!target) return;
    // Look up the node object the force graph is tracking so we
    // read its current (x,y,z) — it'll have been placed by the
    // force simulation after initial ticks.
    const live = (graph.graphData()?.nodes ?? []).find(
      (n: any) => n.__index === idx,
    );
    if (!live || typeof live.x !== 'number') return;
    const distance = 80;
    const distRatio = 1 + distance / Math.hypot(live.x, live.y, live.z || 1);
    graph.cameraPosition(
      { x: live.x * distRatio, y: live.y * distRatio, z: (live.z ?? 0) * distRatio },
      { x: live.x, y: live.y, z: live.z ?? 0 },
      1400,
    );
  }

  onCleanup(() => {
    cancelAnimationFrame(rafHandle);
    if (graph) graph._destructor?.();
  });

  return (
    <div
      ref={container}
      style={{
        width: '100%',
        height: '100%',
        position: 'absolute',
        inset: '0',
      }}
    />
  );
}

function escape(s: string): string {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}
