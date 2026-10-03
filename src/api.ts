import { invoke } from '@tauri-apps/api/core';

export type EngineId = 'yandex' | 'bing' | 'google' | 'mymemory' | 'libre';

export const ENGINES: Array<{ id: EngineId; name: string; desc: string }> = [
  { id: 'bing', name: 'Microsoft Bing', desc: 'Онлайн · без ключа' },
  { id: 'google', name: 'Google', desc: 'Онлайн · без ключа · иногда лимит' },
  { id: 'yandex', name: 'Яндекс', desc: 'Онлайн · без ключа (переводит некорректно)' },
  { id: 'mymemory', name: 'MyMemory', desc: 'Онлайн · бесплатно с лимитом' },
  { id: 'libre', name: 'LibreTranslate', desc: 'Онлайн · свой/публичный сервер' },
];

export function isEngine(id: string): id is EngineId {
  return ENGINES.some((e) => e.id === id);
}

export type OcrEngineId = 'system' | 'builtin';

export function isOcrEngine(id: string): id is OcrEngineId {
  return id === 'system' || id === 'builtin';
}

export interface TrcSettings {
  enabled: boolean;
  engine: EngineId;
  ocrEngine: OcrEngineId; // 'system' (как в «Ножницах») | 'builtin' (tesseract)
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

export interface UpdateInfo {
  current: string;
  latest: string;
  download_url: string;
  size: number;
}

export interface UpdateProgress {
  downloaded: number;
  total: number;
  percent: number;
  speed_mbps: number;
}

export const DEFAULT_SETTINGS: TrcSettings = {
  enabled: true,
  engine: 'bing',
  ocrEngine: 'system',
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

// Автоопределение языка текста по алфавиту: любая весомая кириллица → ru,
// иначе весомая латиница → en, иначе null (цифры/символы — не переключаем).
export function detectLang(text: string): 'ru' | 'en' | null {
  let cyr = 0;
  let lat = 0;
  for (const c of text) {
    if (/[а-яё]/i.test(c)) cyr++;
    else if (/[a-z]/i.test(c)) lat++;
  }
  if (cyr >= 3) return 'ru';
  if (lat >= 3) return 'en';
  return null;
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
  // Системный OCR как в «Ножницах» (Windows.Media.Ocr): base64 PNG без префикса, язык 'en'|'ru'.
  ocrImage: (imageBase64: string, lang: string) => invoke<string>('ocr_image', { imageBase64, lang }),
  ocrAvailable: () => invoke<void>('ocr_available'),
  closeSelector: (restore: boolean = true) => invoke<void>('close_selector', { restore }),
  setHotkeys: (overlay: string, translate: string, capture: string) =>
    invoke<void>('set_hotkeys', { overlay, translate, capture }),
  showMain: () => invoke<void>('show_main'),
  hideToTray: () => invoke<void>('hide_to_tray'),
  quitApp: () => invoke<void>('quit_app'),
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  downloadUpdate: (url: string) => invoke<string>('download_update', { url }),
  applyUpdate: () => invoke<void>('apply_update'),
};
