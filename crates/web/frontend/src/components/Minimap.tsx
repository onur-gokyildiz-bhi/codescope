import { onMount, createEffect, onCleanup } from 'solid-js';
import { graphData, setCenterNode } from '../store';
import { kindColor } from '../utils/colors';

export default function Minimap() {
  let canvasRef!: HTMLCanvasElement;
  let animFrame: number;

  onMount(() => {
    draw();
  });

  createEffect(() => {
    graphData(); // track reactivity
    draw();
  });

  function draw() {
    const canvas = canvasRef;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const w = 140;
    const h = 100;
    canvas.width = w * 2; // retina
    canvas.height = h * 2;
    ctx.scale(2, 2);

    ctx.fillStyle = '#0d1117';
    ctx.fillRect(0, 0, w, h);

    const nodes = graphData().nodes;
    if (!nodes.length) return;

    // Find bounds from node positions
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const n of nodes) {
      const x = n.x ?? 0;
      const y = n.y ?? 0;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }

    const rangeX = maxX - minX || 1;
    const rangeY = maxY - minY || 1;
    const pad = 8;

    for (const n of nodes) {
      const px = pad + ((n.x ?? 0) - minX) / rangeX * (w - pad * 2);
      const py = pad + ((n.y ?? 0) - minY) / rangeY * (h - pad * 2);
      ctx.fillStyle = kindColor(n);
      ctx.globalAlpha = 0.7;
      ctx.beginPath();
      ctx.arc(px, py, 1.5, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }

  function onClick(e: MouseEvent) {
    const rect = canvasRef.getBoundingClientRect();
    const x = (e.clientX - rect.left) / rect.width;
    const y = (e.clientY - rect.top) / rect.height;

    // Find closest node
    const nodes = graphData().nodes;
    if (!nodes.length) return;

    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const n of nodes) {
      if ((n.x ?? 0) < minX) minX = n.x ?? 0;
      if ((n.x ?? 0) > maxX) maxX = n.x ?? 0;
      if ((n.y ?? 0) < minY) minY = n.y ?? 0;
      if ((n.y ?? 0) > maxY) maxY = n.y ?? 0;
    }

    const targetX = minX + x * (maxX - minX);
    const targetY = minY + y * (maxY - minY);

    let closest: any = null;
    let closestDist = Infinity;
    for (const n of nodes) {
      const dx = (n.x ?? 0) - targetX;
      const dy = (n.y ?? 0) - targetY;
      const dist = dx * dx + dy * dy;
      if (dist < closestDist) {
        closestDist = dist;
        closest = n;
      }
    }

    if (closest) {
      setCenterNode(closest.name);
    }
  }

  onCleanup(() => {
    if (animFrame) cancelAnimationFrame(animFrame);
  });

  return (
    <div class="minimap panel">
      <canvas ref={canvasRef} onClick={onClick} />
    </div>
  );
}
