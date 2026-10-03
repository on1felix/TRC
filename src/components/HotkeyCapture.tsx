import { useEffect, useState } from 'react';

// Канонические токены для tauri-plugin-global-shortcut (регистр не важен):
// NUMPAD1.., DIGIT1.., KEYT.., F8.., SPACE, ENTER, ARROWLEFT.. и т.д.
// Мышь: MouseX1 / MouseX2 / MouseMiddle (обрабатывает Rust-хук, не плагин).

const MODS = ['ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight'];

function baseFromCode(code: string): string | null {
  if (MODS.includes(code)) return null; // голый модификатор — ждём дальше
  return code; // Numpad1, Digit1, KeyT, F8, Space, Escape, ArrowLeft, PrintScreen...
}

export function fmtKey(e: KeyboardEvent): string | null {
  const base = baseFromCode(e.code);
  if (!base) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Super');
  parts.push(base);
  return parts.join('+');
}

const BASE_NAMES: Record<string, string> = {
  Space: 'Пробел',
  Escape: 'Esc',
  Enter: 'Enter',
  Tab: 'Tab',
  Backspace: 'Backspace',
  Delete: 'Del',
  Insert: 'Ins',
  Home: 'Home',
  End: 'End',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
  PrintScreen: 'PrtSc',
  ScrollLock: 'ScrLk',
  NumLock: 'NumLk',
  CapsLock: 'Caps',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  MouseX1: 'Мышь X1',
  MouseX2: 'Мышь X2',
  MouseMiddle: 'Мышь Mid',
};

function friendlyBase(b: string): string {
  if (BASE_NAMES[b]) return BASE_NAMES[b];
  if (b.startsWith('Numpad')) {
    const rest = b.slice(6);
    const map: Record<string, string> = {
      Add: '+',
      Subtract: '-',
      Multiply: '*',
      Divide: '/',
      Decimal: '.',
      Enter: 'Enter',
    };
    return 'Num ' + (map[rest] ?? rest);
  }
  if (b.startsWith('Digit')) return b.slice(5);
  if (b.startsWith('Key')) return b.slice(3);
  return b;
}

export function friendlyHotkey(value: string): string {
  if (!value) return '—';
  return value
    .split('+')
    .map((p) => {
      const low = p.toLowerCase();
      if (low === 'ctrl' || low === 'control') return 'Ctrl';
      if (low === 'alt') return 'Alt';
      if (low === 'shift') return 'Shift';
      if (low === 'super' || low === 'meta' || low === 'win') return 'Win';
      return friendlyBase(p);
    })
    .join(' + ');
}

export function HotkeyCapture({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const s = fmtKey(e);
      if (s) {
        onChange(s);
        setListening(false);
      }
    };
    const onMouse = (e: MouseEvent) => {
      // ЛКМ/ПКМ не биндим (сломают клики), только Mid и боковые.
      let token: string | null = null;
      if (e.button === 1) token = 'MouseMiddle';
      else if (e.button === 3) token = 'MouseX1';
      else if (e.button === 4) token = 'MouseX2';
      if (token) {
        e.preventDefault();
        e.stopPropagation();
        onChange(token);
        setListening(false);
      }
    };
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('mousedown', onMouse, true);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('mousedown', onMouse, true);
    };
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
      {listening ? 'Клавиша / кнопка мыши…' : friendlyHotkey(value)}
    </button>
  );
}
