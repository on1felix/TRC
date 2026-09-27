import { create } from 'zustand';
import { api, DEFAULT_SETTINGS, isEngine, type TrcSettings } from './api';

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
      if (!isEngine(s.engine)) s.engine = 'yandex';
      if (s.source !== 'en' && s.source !== 'ru') s.source = 'en';
      if (s.target !== 'en' && s.target !== 'ru') s.target = 'ru';
      set({ ...s, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },

  patch: async (p) => {
    set(p as Partial<TrcStore>);
    // Сохраняем сразу, без дебаунса: иначе перевод, нажатый в ту же секунду,
    // уходил со старым движком, а UI «откатывался» к нему обратно.
    const { loaded, status, load, patch, applyRemote, setStatus, ...s } = get();
    void loaded; void status; void load; void patch; void applyRemote; void setStatus;
    try {
      await api.saveSettings(s as TrcSettings);
      const st = s as TrcSettings;
      await api.setHotkeys(st.overlayHotkey, st.translateHotkey, st.captureHotkey);
    } catch {
      /* ignore */
    }
  },

  applyRemote: (s) => set({ ...s }),
  setStatus: (s) => set({ status: s }),
}));
