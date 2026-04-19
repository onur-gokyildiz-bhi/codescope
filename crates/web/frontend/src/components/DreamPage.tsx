// Phase 3 Dream — iter 2.
//
// Three-zone layout for the /dream route:
//   left   — arc list (tag-derived), most-recent first;
//   center — scene carousel + narration card for the active arc;
//            the 3D tour is stitched in by iter 3 (Graph3D.tsx props).
//   right  — reserved for scene metadata / node deep-links.
//
// All data comes from /api/dream/*. We cache the arcs list at mount
// so tab switches don't refetch, but refetch when the project
// selector changes (tracked by `currentProject()`).

import { createEffect, createResource, createSignal, For, Show } from 'solid-js';
import { Moon, PlayCircle, PauseCircle, SkipForward, SkipBack, Download } from 'lucide-solid';
import { api, type DreamArcSummary, type DreamArcDetail, type DreamScene } from '../api';
import { currentProject } from '../store';
import DreamGraph3D from './DreamGraph3D';

export default function DreamPage() {
  const [activeArcId, setActiveArcId] = createSignal<string | null>(null);
  const [sceneIdx, setSceneIdx] = createSignal(0);
  const [autoplay, setAutoplay] = createSignal(false);

  // Arcs resource — refetches when the project changes.
  const [arcs] = createResource(currentProject, async () => {
    const res = await api.dreamArcs().catch((e) => {
      console.error('dreamArcs failed:', e);
      return { arcs: [] as DreamArcSummary[] };
    });
    return res.arcs;
  });

  // Detail for the active arc — refetches when activeArcId changes.
  const [arcDetail] = createResource(activeArcId, async (id) => {
    if (!id) return null;
    try {
      return await api.dreamArc(id);
    } catch (e) {
      console.error(`dreamArc(${id}) failed:`, e);
      return null;
    }
  });

  // Auto-select the first arc once arcs land.
  createEffect(() => {
    const list = arcs();
    if (list && list.length > 0 && !activeArcId()) {
      setActiveArcId(list[0].id);
      setSceneIdx(0);
    }
  });

  // Reset scene index when switching arc.
  createEffect(() => {
    const id = activeArcId();
    if (id) setSceneIdx(0);
  });

  // Autoplay: advance every 6s. Stops when scenes run out or the
  // user manually scrubs via skip.
  createEffect(() => {
    if (!autoplay()) return;
    const total = arcDetail()?.scenes.length ?? 0;
    if (total === 0) return;
    const handle = setInterval(() => {
      const next = sceneIdx() + 1;
      if (next >= total) {
        setAutoplay(false);
        return;
      }
      setSceneIdx(next);
    }, 6000);
    return () => clearInterval(handle);
  });

  const activeScene = (): DreamScene | null => {
    const detail = arcDetail();
    if (!detail) return null;
    return detail.scenes[sceneIdx()] ?? null;
  };

  const skip = (delta: number) => {
    const total = arcDetail()?.scenes.length ?? 0;
    if (total === 0) return;
    setSceneIdx((i) => Math.max(0, Math.min(total - 1, i + delta)));
  };

  const exportMarkdown = () => {
    const detail = arcDetail();
    if (!detail) return;
    const md = arcToMarkdown(detail);
    const blob = new Blob([md], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `dream-${safeFileName(detail.id)}.md`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <div class="dream-layout">
      {/* Full-bleed 3D tour graph renders behind every panel */}
      <Show when={(arcDetail()?.scenes.length ?? 0) > 0}>
        <DreamGraph3D
          scenes={arcDetail()!.scenes}
          currentIndex={sceneIdx()}
          onNodeClick={(i) => setSceneIdx(i)}
        />
      </Show>

      {/* Left — arc list */}
      <aside class="dream-arcs">
        <div class="dream-arcs-header">
          <Moon size={18} />
          <span>Arcs</span>
        </div>
        <Show when={!arcs.loading} fallback={<div class="dream-status">Loading arcs…</div>}>
          <Show
            when={(arcs() ?? []).length > 0}
            fallback={<div class="dream-status">No arcs yet — tag some knowledge entries to see them here.</div>}
          >
            <ul class="dream-arcs-list">
              <For each={arcs() ?? []}>
                {(arc) => (
                  <li
                    class="dream-arc-row"
                    classList={{ 'dream-arc-row--active': arc.id === activeArcId() }}
                    onClick={() => setActiveArcId(arc.id)}
                  >
                    <div class="dream-arc-title">{arc.title}</div>
                    <div class="dream-arc-meta">
                      {arc.count} scenes · {formatRange(arc.first_at, arc.last_at)}
                    </div>
                    <div class="dream-arc-kinds">
                      <For each={arc.kinds}>{(k) => <span class="dream-chip">{k}</span>}</For>
                    </div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
      </aside>

      {/* Center — narration + transport. The 3D tour will sit
          beneath these controls once iter 3 lands. */}
      <section class="dream-stage">
        <Show when={arcDetail()} fallback={<div class="dream-stage-empty">Select an arc on the left.</div>}>
          {(detail) => (
            <>
              <header class="dream-stage-header">
                <div>
                  <div class="dream-stage-title">{detail().title}</div>
                  <div class="dream-stage-sub">{detail().scenes.length} scenes</div>
                </div>
                <div class="dream-controls">
                  <button onClick={() => skip(-1)} aria-label="Previous scene" title="Previous">
                    <SkipBack size={18} />
                  </button>
                  <button onClick={() => setAutoplay((v) => !v)} aria-label="Play/pause" title="Play / pause (6s per scene)">
                    {autoplay() ? <PauseCircle size={22} /> : <PlayCircle size={22} />}
                  </button>
                  <button onClick={() => skip(1)} aria-label="Next scene" title="Next">
                    <SkipForward size={18} />
                  </button>
                  <button onClick={exportMarkdown} aria-label="Export arc as markdown" title="Export as markdown">
                    <Download size={18} />
                  </button>
                </div>
              </header>

              {/* Scene rail — click any to jump */}
              <div class="dream-rail">
                <For each={detail().scenes}>
                  {(scene, i) => (
                    <button
                      class="dream-rail-dot"
                      classList={{ 'dream-rail-dot--active': i() === sceneIdx() }}
                      onClick={() => setSceneIdx(i())}
                      title={scene.title}
                    />
                  )}
                </For>
              </div>

              {/* Active narration card */}
              <Show when={activeScene()} fallback={<div class="dream-stage-empty">Empty arc.</div>}>
                {(scene) => (
                  <article class="dream-scene-card">
                    <div class="dream-scene-meta">
                      <span class="dream-chip dream-chip--kind">{scene().kind}</span>
                      <Show when={scene().created_at}>
                        <span class="dream-scene-date">
                          {scene().created_at?.split('T')[0]}
                        </span>
                      </Show>
                      <span class="dream-scene-index">
                        {sceneIdx() + 1} / {detail().scenes.length}
                      </span>
                    </div>
                    <h2 class="dream-scene-title">{scene().title}</h2>
                    <p class="dream-scene-narration">{scene().narration}</p>
                    <Show when={scene().content}>
                      <pre class="dream-scene-body">{excerpt(scene().content, 1200)}</pre>
                    </Show>
                  </article>
                )}
              </Show>
            </>
          )}
        </Show>
      </section>
    </div>
  );
}

function formatRange(first: string | null, last: string | null): string {
  if (!first && !last) return 'unknown';
  const f = first?.split('T')[0] ?? '?';
  const l = last?.split('T')[0] ?? '?';
  return f === l ? f : `${f} → ${l}`;
}

function excerpt(content: string, max: number): string {
  const t = content.trim();
  if (t.length <= max) return t;
  return t.slice(0, max) + '…';
}

/// Serialise an arc to a standalone markdown memoir. Format is
/// intentionally plain — one H1 header, a date-range line, then
/// each scene as an H2 with narration + a fenced content block so
/// the file reads cleanly in any viewer.
function arcToMarkdown(detail: DreamArcDetail): string {
  const scenes = detail.scenes ?? [];
  const first = scenes[0]?.created_at?.split('T')[0];
  const last = scenes[scenes.length - 1]?.created_at?.split('T')[0];
  const range = first && last
    ? (first === last ? first : `${first} → ${last}`)
    : 'unknown';

  const lines: string[] = [];
  lines.push(`# ${detail.title}`);
  lines.push('');
  lines.push(`*${scenes.length} scenes · ${range} · tag: \`${detail.tag}\`*`);
  lines.push('');
  lines.push('---');
  lines.push('');
  scenes.forEach((s, i) => {
    const date = s.created_at?.split('T')[0] ?? '—';
    lines.push(`## ${i + 1}. ${s.title}`);
    lines.push('');
    lines.push(`*${date} · ${s.kind}*`);
    lines.push('');
    lines.push(s.narration);
    lines.push('');
    if (s.content && s.content.trim()) {
      lines.push('<details>');
      lines.push('<summary>content</summary>');
      lines.push('');
      lines.push(s.content.trim());
      lines.push('');
      lines.push('</details>');
      lines.push('');
    }
    if (i + 1 < scenes.length) {
      lines.push('---');
      lines.push('');
    }
  });
  lines.push('');
  lines.push(`_Exported from Codescope Dream · ${new Date().toISOString()}_`);
  lines.push('');
  return lines.join('\n');
}

/// Filesystem-safe slug for the download name. Keeps alphanumerics,
/// dash, dot; collapses everything else to a single dash.
function safeFileName(id: string): string {
  return id.replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '') || 'arc';
}
