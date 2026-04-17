import { onMount, onCleanup } from 'solid-js';
import { Moon } from 'lucide-solid';

export default function DreamPlaceholder() {
  let canvasRef!: HTMLCanvasElement;
  let animHandle = 0;

  onMount(() => {
    const canvas = canvasRef;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    function resize() {
      canvas.width = canvas.offsetWidth;
      canvas.height = canvas.offsetHeight;
    }
    resize();
    window.addEventListener('resize', resize);

    // Particle starfield
    const STAR_COUNT = 120;
    const stars = Array.from({ length: STAR_COUNT }, () => ({
      x: Math.random(),
      y: Math.random(),
      r: Math.random() * 1.5 + 0.3,
      speed: Math.random() * 0.00015 + 0.00005,
      opacity: Math.random() * 0.7 + 0.2,
      twinkle: Math.random() * Math.PI * 2,
    }));

    function draw(t: number) {
      const w = canvas.width;
      const h = canvas.height;
      ctx!.clearRect(0, 0, w, h);
      for (const s of stars) {
        s.twinkle += s.speed * 60;
        const alpha = s.opacity * (0.6 + 0.4 * Math.sin(s.twinkle));
        ctx!.beginPath();
        ctx!.arc(s.x * w, s.y * h, s.r, 0, Math.PI * 2);
        ctx!.fillStyle = `rgba(163,113,247,${alpha.toFixed(3)})`;
        ctx!.fill();
      }
      animHandle = requestAnimationFrame(draw);
    }
    animHandle = requestAnimationFrame(draw);

    onCleanup(() => {
      cancelAnimationFrame(animHandle);
      window.removeEventListener('resize', resize);
    });
  });

  return (
    <div style={{
      display: 'flex',
      'flex-direction': 'column',
      'align-items': 'center',
      'justify-content': 'center',
      height: '100%',
      gap: 'var(--space-4)',
      position: 'relative',
    }}>
      {/* Starfield background */}
      <canvas
        ref={canvasRef!}
        style={{
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          'pointer-events': 'none',
          opacity: '0.6',
        }}
      />

      {/* Content */}
      <div style={{ position: 'relative', 'z-index': 1, 'text-align': 'center', display: 'flex', 'flex-direction': 'column', 'align-items': 'center', gap: 'var(--space-4)' }}>
        <Moon
          size={48}
          stroke-width={1.5}
          style={{ color: 'var(--accent-violet)', filter: 'drop-shadow(0 0 12px rgba(163,113,247,0.6))' }}
        />
        <div>
          <div style={{ 'font-size': 'var(--font-xl)', 'font-weight': '700', 'margin-bottom': 'var(--space-2)', color: 'var(--accent-violet)' }}>
            /dream
          </div>
          <div style={{ 'font-size': 'var(--font-md)', color: 'var(--text-dim)' }}>
            Coming soon — Phase 3
          </div>
        </div>
      </div>
    </div>
  );
}
