// CMX-01b — insight view.
//
// Reads /api/insight and renders the same three numbers the CLI
// `codescope insight` prints: total calls + estimated tokens
// saved, per-repo bar chart, hourly sparkline. One auto-refresh
// every 30 s so the dashboard stays live without the user
// having to reload.

import { createEffect, createResource, onCleanup, For, Show } from 'solid-js';
import { BarChart3, Activity, Zap, Clock } from 'lucide-solid';
import { api, type InsightResponse, type SessionRecap } from '../api';

export default function InsightPage() {
  const [data, { refetch }] = createResource<InsightResponse>(async () => {
    try {
      return await api.insight();
    } catch (e) {
      console.error('insight fetch failed:', e);
      return { summary: { total_calls: 0, repos: {}, hours: {}, first_ts: null, last_ts: null, by_kind: {} }, gain: { total_calls: 0, tokens_per_call_est: 2500, tokens_saved_est: 0, first_used: null, last_used: null } };
    }
  });

  const [sessions, { refetch: refetchSessions }] = createResource<SessionRecap[]>(async () => {
    try {
      return (await api.sessionRecent(5)).sessions;
    } catch (e) {
      console.error('session fetch failed:', e);
      return [];
    }
  });

  // 30 s auto-refresh. The MCP server flushes every 30 s too, so
  // this matches freshness.
  createEffect(() => {
    const handle = setInterval(() => {
      refetch();
      refetchSessions();
    }, 30_000);
    onCleanup(() => clearInterval(handle));
  });

  return (
    <div class="insight-layout">
      <Show
        when={data()}
        fallback={<div class="insight-status">Loading insight…</div>}
      >
        {(d) => (
          <>
            {/* Headline row — 3 big numbers */}
            <div class="insight-row">
              <HeadlineCard
                icon={Activity}
                label="Total tool calls"
                value={formatNum(d().summary.total_calls)}
                sub={formatRange(d().summary.first_ts, d().summary.last_ts)}
              />
              <HeadlineCard
                icon={Zap}
                label="Tokens saved (est.)"
                value={`~${formatNum(d().gain.tokens_saved_est)}`}
                sub={`≈ ${d().gain.tokens_per_call_est} / call`}
                accent="green"
              />
              <HeadlineCard
                icon={BarChart3}
                label="Repos touched"
                value={String(Object.keys(d().summary.repos).length)}
                sub="distinct projects queried"
              />
            </div>

            {/* Per-repo bar chart */}
            <section class="insight-section">
              <h3>By repo</h3>
              <RepoBars repos={d().summary.repos} />
            </section>

            {/* Hourly sparkline — last 24 active buckets */}
            <section class="insight-section">
              <h3>Recent activity <span class="insight-muted">(last 24 active hours)</span></h3>
              <Sparkline hours={d().summary.hours} />
            </section>

            {/* CMX-02 — recent sessions */}
            <section class="insight-section">
              <h3>Sessions <span class="insight-muted">(last 5)</span></h3>
              <SessionsList sessions={sessions() ?? []} />
            </section>
          </>
        )}
      </Show>
    </div>
  );
}

function SessionsList(props: { sessions: SessionRecap[] }) {
  return (
    <Show
      when={props.sessions.length > 0}
      fallback={<div class="insight-status">No sessions recorded yet.</div>}
    >
      <div class="insight-sessions">
        <For each={props.sessions}>
          {(s) => {
            const dur = s.ended_at - s.started_at;
            const byKind = Object.entries(s.kinds ?? {}) as [string, number][];
            return (
              <div class="insight-session">
                <div class="insight-session-head">
                  <Clock size={12} />
                  <span class="insight-session-id">{s.session_id}</span>
                  <span class="insight-session-dur">{formatDur(dur)}</span>
                </div>
                <div class="insight-session-meta">
                  <span>{formatNum(s.event_count)} events</span>
                  <span>
                    {s.repos.length > 0 ? s.repos.join(', ') : '—'}
                  </span>
                  <For each={byKind}>
                    {([k, n]) => (
                      <span
                        class="insight-session-kind"
                        classList={{ 'is-error': k === 'error' }}
                      >
                        {k}={n}
                      </span>
                    )}
                  </For>
                </div>
                <ol class="insight-session-tail">
                  <For each={s.tail.slice(-5)}>
                    {(ev) => (
                      <li classList={{ 'is-error': ev.kind === 'error' }}>
                        <span class="insight-session-icon">
                          {ev.kind === 'tool_call'
                            ? '›'
                            : ev.kind === 'file_edit'
                            ? '✎'
                            : '✗'}
                        </span>
                        <span class="insight-session-time">{hms(ev.ts)}</span>
                        <span class="insight-session-repo">{ev.repo}</span>
                        {ev.detail ? (
                          <span class="insight-session-detail">{ev.detail}</span>
                        ) : null}
                      </li>
                    )}
                  </For>
                </ol>
              </div>
            );
          }}
        </For>
      </div>
    </Show>
  );
}

function formatDur(secs: number): string {
  if (secs <= 0) return '<1s';
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m${secs % 60}s`;
  const h = Math.floor(secs / 3600);
  const rem = secs % 3600;
  return `${h}h${Math.floor(rem / 60)}m`;
}

function hms(ts: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

function HeadlineCard(props: {
  icon: any;
  label: string;
  value: string;
  sub: string;
  accent?: 'green';
}) {
  const Icon = props.icon;
  return (
    <div class="insight-card" classList={{ 'insight-card--green': props.accent === 'green' }}>
      <div class="insight-card-head">
        <Icon size={16} />
        <span>{props.label}</span>
      </div>
      <div class="insight-card-value">{props.value}</div>
      <div class="insight-card-sub">{props.sub}</div>
    </div>
  );
}

function RepoBars(props: { repos: Record<string, number> }) {
  const entries = (): [string, number][] =>
    Object.entries(props.repos).sort(([, a], [, b]) => b - a).slice(0, 20);
  const max = (): number =>
    entries().reduce((m, [, n]) => Math.max(m, n), 1);
  return (
    <div class="insight-bars">
      <For each={entries()} fallback={<div class="insight-status">No events yet.</div>}>
        {([name, n]) => {
          const pct = () => Math.max(3, Math.round((n / max()) * 100));
          return (
            <div class="insight-bar-row">
              <div class="insight-bar-name">{name}</div>
              <div class="insight-bar-track">
                <div class="insight-bar-fill" style={{ width: `${pct()}%` }} />
              </div>
              <div class="insight-bar-num">{formatNum(n)}</div>
            </div>
          );
        }}
      </For>
    </div>
  );
}

function Sparkline(props: { hours: Record<string, number> }) {
  const bars = (): number[] => {
    const keys = Object.keys(props.hours).sort();
    const tail = keys.slice(-24).map((k) => props.hours[k]);
    return tail;
  };
  const max = (): number => Math.max(1, ...bars());
  return (
    <div class="insight-spark">
      <For each={bars()} fallback={<div class="insight-status">No hourly data.</div>}>
        {(n) => {
          const h = () => Math.max(4, Math.round((n / max()) * 56));
          return (
            <div class="insight-spark-bar-wrap" title={`${n} calls`}>
              <div class="insight-spark-bar" style={{ height: `${h()}px` }} />
            </div>
          );
        }}
      </For>
    </div>
  );
}

function formatNum(n: number): string {
  if (!Number.isFinite(n)) return '0';
  return new Intl.NumberFormat('en-US').format(Math.floor(n));
}

function formatRange(first: number | null, last: number | null): string {
  if (!first || !last) return 'no events yet';
  const f = new Date(first * 1000).toISOString().slice(0, 10);
  const l = new Date(last * 1000).toISOString().slice(0, 10);
  return f === l ? f : `${f} → ${l}`;
}
