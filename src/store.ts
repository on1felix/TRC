import { create } from 'zustand';
import { api, DEFAULT_SETTINGS, isEngine, isOcrEngine, type TrcSettings } from './api';

interface TrcStore extends TrcSettings {
  loaded: boolean;
  status: string;

  load: () => Promise<void>;
  patch: (p: Partial<TrcSettings>) => Promise<void>;
  applyRemote: (s: TrcSettings) => void;
  setStatus: (s: string) => void;
}

export const useTrc = create<TrcStore>((set, get) => ({
  ...DEFAULT_SETTINGS,
  loaded: false,
  status: '',

  load: async () => {
    try {
      const s = await api.getSettings();
      if (!isEngine(s.engine)) s.engine = 'bing';
      if (!isOcrEngine(s.ocrEngine)) s.ocrEngine = 'system';
      if (s.source !== 'en' && s.source !== 'ru') s.source = 'en';
      if (s.target !== 'en' && s.target !== 'ru') s.target = 'ru';
      set({ ...s, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },

  patch: async (p) => {
    // Read-modify-write: два окна (главное + панель) делят одни настройки,
    // поэтому сначала забираем свежие из бэкенда — иначе одно окно затрёт
    // изменения другого (напр. тумблер «переводить сразу»).
    // После записи перечитываем и сверяем: молчаливых несейвов больше нет.
    const cur = get();
    let base: TrcSettings;
    try {
      base = await api.getSettings();
    } catch {
      const { loaded, status, load, patch, applyRemote, setStatus, ...rest } = cur;
      void loaded; void status; void load; void patch; void applyRemote; void setStatus;
      base = rest as TrcSettings;
    }
    const merged = { ...base, ...p };
    set(merged as Partial<TrcStore>);
    try {
      await api.saveSettings(merged);
      const check = await api.getSettings();
      const keys = Object.keys(p) as Array<keyof TrcSettings>;
      const bad = keys.filter((k) => JSON.stringify(check[k]) !== JSON.stringify(merged[k]));
      if (bad.length > 0) {
        // одна повторная попытка, потом — видимая ошибка
        await api.saveSettings(merged);
        const recheck = await api.getSettings();
        const stillBad = keys.filter((k) => JSON.stringify(recheck[k]) !== JSON.stringify(merged[k]));
        if (stillBad.length > 0) throw new Error('mismatch: ' + stillBad.join(','));
      }
      set({ status: '' });
    } catch (e) {
      set({ status: `Настройки не сохранились: ${String(e)}` });
    }
  },

  applyRemote: (s) => set({ ...s }),
  setStatus: (s) => set({ status: s }),
}));
