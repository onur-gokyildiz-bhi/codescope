import { Show } from 'solid-js';
import { selectedNode } from '../store';
import NodeInspector from './NodeInspector';

export default function RightPanel() {
  return (
    <Show when={selectedNode()}>
      <div class="right-panel panel">
        <NodeInspector />
      </div>
    </Show>
  );
}
