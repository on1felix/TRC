import { useEffect, useState } from 'react';

function fmt(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  const k = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (!['Control', 'Alt', 'Shift'].includes(e.key)) parts.push(k);
  return parts.join('+');
}

export function HotkeyCapture({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const h = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const s = fmt(e);
      if (s && !['Ctrl', 'Alt', 'Shift'].includes(s)) {
        onChange(s);
        setListening(false);
      }
    };
    window.addEventListener('keydown', h, true);
    return () => window.removeEventListener('keydown', h, true);
  }, [listening, onChange]);

  return (
    <button
      onClick={() => setListening(true)}
      className={`px-3 py-1.5 rounded-md border text-xs font-mono transition-colors ${
        listening
          ? 'border-accent text-accent animate-pulse'
          : 'border-border bg-white/[0.04] text-text-2 hover:border-accent/60'
      }`}
    >
      {listening ? 'Нажмите клавиши…' : value || '—'}
    </button>
  );
}
