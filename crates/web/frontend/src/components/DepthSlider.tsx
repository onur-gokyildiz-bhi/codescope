import { graphDepth, setGraphDepth } from '../store';

export default function DepthSlider() {
  return (
    <div class="control-group">
      <span class="control-label">Depth</span>
      <input
        class="control-slider"
        type="range"
        min="1"
        max="4"
        step="1"
        value={graphDepth()}
        onInput={e => setGraphDepth(parseInt(e.currentTarget.value))}
      />
      <span class="control-value">{graphDepth()}</span>
    </div>
  );
}
