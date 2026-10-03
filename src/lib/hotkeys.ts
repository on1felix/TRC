// Глобальные хоткеи.
// Клавиатура — через JS-плагин tauri-plugin-global-shortcut.
// Кнопки мыши (MouseX1/MouseX2/MouseMiddle) — через нативный хук в Rust,
// сюда приходят событиями 'trc:mouse-hotkey'.
// Работают даже когда игра в фокусе и когда главное окно скрыто в трей.

import { register, unregisterAll } from '@tauri-apps/plugin-global-shortcut';
import { api } from '../api';

export function isMouseHotkey(h: string): boolean {
  return /^mouse/i.test((h || '').trim());
}

function toPluginFormat(h: string): string {
  return (h || '').replace(/\s+/g, '');
}

export async function reregisterHotkeys(overlay: string, capture: string) {
  try {
    await unregisterAll();
  } catch {
    /* ignore */
  }
  const used = new Set<string>();
  const take = async (key: string, fn: () => void) => {
    const k = toPluginFormat(key);
    if (!k || used.has(k.toLowerCase()) || isMouseHotkey(k)) return;
    used.add(k.toLowerCase());
    try {
      await register(k, (e) => {
        if (e.state === 'Pressed') fn();
      });
    } catch {
      /* невалидный хоткей — пропускаем */
    }
  };
  await take(overlay, () => void api.togglePanel());
  await take(capture, () => void api.prepareSelection().catch(() => {}));
}
