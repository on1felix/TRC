// Глобальные хоткеи через JS-плагин tauri-plugin-global-shortcut.
// Работают даже когда игра в фокусе и когда главное окно скрыто в трей.

import { register, unregisterAll } from '@tauri-apps/plugin-global-shortcut';
import { emit } from '@tauri-apps/api/event';
import { api } from '../api';

function toPluginFormat(h: string): string {
  return (h || '').replace(/\s+/g, '');
}

export async function reregisterHotkeys(overlay: string, translate: string, capture: string) {
  try {
    await unregisterAll();
  } catch {
    /* ignore */
  }
  const o = toPluginFormat(overlay);
  const t = toPluginFormat(translate);
  const c = toPluginFormat(capture);
  const used = new Set<string>();
  const take = async (key: string, fn: () => void) => {
    if (!key || used.has(key)) return;
    used.add(key);
    try {
      await register(key, (e) => {
        if (e.state === 'Pressed') fn();
      });
    } catch {
      /* невалидный хоткей — пропускаем */
    }
  };
  await take(o, () => void api.togglePanel());
  await take(t, () => void emit('trc:translate-now'));
  await take(c, () => void api.prepareSelection().catch(() => {}));
}
