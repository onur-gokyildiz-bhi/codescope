export default function DreamPlaceholder() {
  return (
    <div style={{
      display: 'flex',
      'flex-direction': 'column',
      'align-items': 'center',
      'justify-content': 'center',
      height: '100%',
      gap: 'var(--space-4)',
    }}>
      <div style={{
        width: '64px',
        height: '64px',
        'border-radius': '50%',
        background: 'conic-gradient(from 0deg, #00e5ff, #ff3df5, #7cff5c, #00e5ff)',
        animation: 'rotateBorder 3s linear infinite',
        opacity: '0.7',
      }} />
      <div style={{ 'text-align': 'center' }}>
        <div style={{ 'font-size': 'var(--font-xl)', 'font-weight': '700', 'margin-bottom': 'var(--space-2)' }}>
          /dream
        </div>
        <div style={{ 'font-size': 'var(--font-md)', color: 'var(--text-dim)' }}>
          Coming soon — Phase 3
        </div>
      </div>
    </div>
  );
}
