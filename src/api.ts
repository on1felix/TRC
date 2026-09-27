import { invoke } from '@tauri-apps/api/core';

export type EngineId = 'yandex' | 'bing' | 'google' | 'mymemory' | 'libre';

export const ENGINES: Array<{ id: EngineId; name: string; desc: string }> = [
  { id: 'yandex', name: 'Яндекс', desc: 'Онлайн · без ключа · быстро' },
  { id: 'bing', name: 'Microsoft Bing', desc: 'Онлайн · без ключа' },
  { id: 'google', name: 'Google', desc: 'Онлайн · без ключа · иногда лимит' },
  { id: 'mymemory', name: 'MyMemory', desc: 'Онлайн · бесплатно с лимитом' },
  { id: 'libre', name: 'LibreTranslate', desc: 'Онлайн · свой/публичный сервер' },
];

export function isEngine(id: string): id is EngineId {
  return ENGINES.some((e) => e.id === id);
}

export interface TrcSettings {
  enabled: boolean;
  engine: EngineId;
  source: string; // 'en' | 'ru'
  target: string; // 'en' | 'ru'
  live: boolean; // переводить как печатаешь
  overlayHotkey: string; // показать/скрыть панель (напр. 'F8')
  translateHotkey: string; // перевести сейчас (напр. 'Ctrl+Alt+T')
  captureHotkey: string; // выделить область (напр. 'F9')
  libreUrl: string;
}

export interface CaptureResult {
  original: string;
  translated: string;
}

export const DEFAULT_SETTINGS: TrcSettings = {
  enabled: true,
  engine: 'yandex',
  source: 'en',
  target: 'ru',
  live: true,
  overlayHotkey: 'F8',
  translateHotkey: 'Ctrl+Alt+T',
  captureHotkey: 'F9',
  libreUrl: 'https://libretranslate.de',
};

export const LANGS = [
  { id: 'en', name: 'Английский' },
  { id: 'ru', name: 'Русский' },
] as const;

export function langName(id: string): string {
  return LANGS.find((l) => l.id === id)?.name ?? id.toUpperCase();
}

export const api = {
  getSettings: () => invoke<TrcSettings>('get_settings'),
  saveSettings: (s: TrcSettings) => invoke<void>('save_settings', { settings: s }),
  translateText: (text: string, engine: EngineId, libreUrl: string, source: string, target: string) =>
    invoke<string>('translate_text', { text, engine, libreUrl, source, target }),
  togglePanel: () => invoke<void>('toggle_panel'),
  showPanel: () => invoke<void>('show_panel'),
  prepareSelection: () => invoke<{ w: number; h: number }>('prepare_selection'),
  takeSelectionImage: () =>
    invoke<{ pngBase64: string; w: number; h: number }>('take_selection_image'),
  closeSelector: () => invoke<void>('close_selector'),
  setHotkeys: (overlay: string, translate: string, capture: string) =>
    invoke<void>('set_hotkeys', { overlay, translate, capture }),
  showMain: () => invoke<void>('show_main'),
  hideToTray: () => invoke<void>('hide_to_tray'),
  quitApp: () => invoke<void>('quit_app'),
};
