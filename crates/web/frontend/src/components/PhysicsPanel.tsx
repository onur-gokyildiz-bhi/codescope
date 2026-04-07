import { createSignal, Show } from 'solid-js';
import { repelStrength, setRepelStrength, linkDistance, setLinkDistance, centerGravity, setCenterGravity } from '../store';

export default function PhysicsPanel() {
  const [open, setOpen] = createSignal(false);

  return (
    <div class="panel" style="width:220px">
      <div class="collapsible-header" onClick={() => setOpen(v => !v)}>
        <span>Physics</span>
        <span>{open() ? '\u25BE' : '\u25B8'}</span>
      </div>
      <Show when={open()}>
        <div class="panel-body">
          <div class="control-group" style="margin-bottom:8px">
            <span class="control-label">Repel</span>
            <input
              class="control-slider"
              type="range"
              min="-200"
              max="-20"
              step="5"
              value={repelStrength()}
              onInput={e => setRepelStrength(parseInt(e.currentTarget.value))}
            />
            <span class="control-value">{repelStrength()}</span>
          </div>
          <div class="control-group" style="margin-bottom:8px">
            <span class="control-label">Link</span>
            <input
              class="control-slider"
              type="range"
              min="10"
              max="150"
              step="5"
              value={linkDistance()}
              onInput={e => setLinkDistance(parseInt(e.currentTarget.value))}
            />
            <span class="control-value">{linkDistance()}</span>
          </div>
          <div class="control-group">
            <span class="control-label">Center</span>
            <input
              class="control-slider"
              type="range"
              min="0"
              max="1"
              step="0.05"
              value={centerGravity()}
              onInput={e => setCenterGravity(parseFloat(e.currentTarget.value))}
            />
            <span class="control-value">{centerGravity().toFixed(2)}</span>
          </div>
        </div>
      </Show>
    </div>
  );
}
