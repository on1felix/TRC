import { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Languages, Keyboard, BellRing, Minimize2, XCircle, PanelTop, ScanSearch, Settings as SettingsIcon, ArrowLeft, Scan } from 'lucide-react';
import { emit } from '@tauri-apps/api/event';
import { api, ENGINES } from '../api';
import { useTrc } from '../store';
import { TitleBar } from '../components/TitleBar';
import { AppBg } from '../components/AppBg';
import { Toggle } from '../components/Toggle';
import { HotkeyCapture } from '../components/HotkeyCapture';
import { TranslatorCard } from '../components/TranslatorCard';
import { reregisterHotkeys } from '../lib/hotkeys';
import { ensureOcr, onOcrStage, type OcrStage } from '../lib/ocr';

export function MainPage() {
  const s = useTrc();
  const [tab, setTab] = useState<'home' | 'settings'>('home');
  const [ocr, setOcr] = useState<{ stage: OcrStage; detail: string }>({ stage: 'idle', detail: '' });

  useEffect(() => {
    // Греем движок OCR заранее, чтобы захват области не ждал загрузку.
    const off = onOcrStage((stage, detail) => setOcr({ stage, detail }));
    ensureOcr().catch(() => {});
    return off;
  }, []);

  useEffect(() => {
    void s.load().then(() => {
      const st = useTrc.getState();
      void reregisterHotkeys(st.overlayHotkey, st.translateHotkey, st.captureHotkey);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!s.loaded) return;
    void reregisterHotkeys(s.overlayHotkey, s.translateHotkey, s.captureHotkey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.overlayHotkey, s.translateHotkey, s.captureHotkey]);

  if (!s.loaded) {
    return (
      <div className="h-screen flex flex-col bg-bg relative">
        <AppBg />
        <div className="relative z-10 flex flex-col h-full">
          <TitleBar />
          <div className="flex-1 flex items-center justify-center text-text-secondary text-sm">Загрузка…</div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col overflow-hidden bg-bg relative">
      <AppBg />
      <div className="relative z-10 flex flex-col h-full">
        <TitleBar />
        {/* шапка: заголовок + шестерёнка */}
        <div className="flex items-center gap-2 px-6 pt-5 pb-1 shrink-0 max-w-[720px] w-full mx-auto">
          {tab === 'settings' ? (
            <button onClick={() => setTab('home')} title="Назад" className="p-2 -ml-2 rounded-lg hover:bg-white/10 transition-colors">
              <ArrowLeft className="w-5 h-5 text-text-secondary" />
            </button>
          ) : null}
          <div className="font-semibold text-[15px]">
            {tab === 'home' ? 'Переводчик' : 'Настройки'}
          </div>
          <button
            onClick={() => setTab(tab === 'home' ? 'settings' : 'home')}
            title="Настройки"
            className={`ml-auto p-2 rounded-lg transition-colors ${tab === 'settings' ? 'bg-accent/20 text-accent' : 'hover:bg-white/10 text-text-secondary'}`}
          >
            <SettingsIcon className="w-5 h-5" />
          </button>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto">
          <div className="p-6 pt-3 space-y-4 max-w-[720px] w-full mx-auto">
            <AnimatePresence mode="wait">
              {tab === 'home' ? (
                <motion.div
                  key="home"
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -8 }}
                  transition={{ duration: 0.18 }}
                  className="space-y-3"
                >
                  <TranslatorCard />
                  {/* быстрые действия */}
                  <div className="grid grid-cols-2 gap-3">
                    <button
                      onClick={() => api.prepareSelection().catch(() => {})}
                      className="flex items-center justify-center gap-2 py-2.5 rounded-xl border border-white/10 bg-white/[0.04] text-xs font-semibold text-text-2 hover:border-accent/60 hover:text-white transition-colors"
                    >
                      <ScanSearch className="w-4 h-4 text-accent" />
                      Выделить ({s.captureHotkey})
                    </button>
                    <button
                      onClick={() => api.showPanel()}
                      className="flex items-center justify-center gap-2 py-2.5 rounded-xl border border-white/10 bg-white/[0.04] text-xs font-semibold text-text-2 hover:border-accent/60 hover:text-white transition-colors"
                    >
                      <PanelTop className="w-4 h-4 text-accent" />
                      Панель ({s.overlayHotkey})
                    </button>
                  </div>
                  <div className="flex items-center justify-center gap-1.5 text-[11px] text-text-muted">
                    <Scan className="w-3.5 h-3.5" />
                    {ocr.stage === 'ready' ? (
                      <span className="text-success">OCR готов</span>
                    ) : ocr.stage === 'error' ? (
                      <span className="text-danger">{ocr.detail || 'OCR не загрузился'}</span>
                    ) : (
                      <span className="text-accent">{ocr.detail || 'Готовлю OCR…'}</span>
                    )}
                  </div>
                </motion.div>
              ) : (
                <motion.div
                  key="settings"
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -8 }}
                  transition={{ duration: 0.18 }}
                  className="space-y-4"
                >
                  {/* Движок */}
                  <div className="card space-y-3">
                    <div className="flex items-center gap-2 font-semibold text-sm">
                      <Languages className="w-4 h-4 text-accent" /> Через что переводить
                    </div>
                    <div className="grid gap-2">
                      {ENGINES.map((e) => (
                        <button
                          key={e.id}
                          onClick={() => s.patch({ engine: e.id })}
                          className={`text-left px-3 py-2.5 rounded-lg border transition-colors ${s.engine === e.id ? 'border-accent/70 bg-accent/10' : 'border-white/10 bg-white/[0.03] hover:border-accent/40'}`}
                        >
                          <div className="text-sm font-medium">{e.name}</div>
                          <div className="text-[11px] text-text-secondary">{e.desc}</div>
                        </button>
                      ))}
                    </div>
                    {s.engine === 'libre' && (
                      <input
                        value={s.libreUrl}
                        onChange={(e) => s.patch({ libreUrl: e.target.value })}
                        placeholder="https://libretranslate.de"
                        className="w-full px-3 py-2 rounded-md bg-white/5 border border-white/10 text-xs"
                      />
                    )}
                  </div>

                  {/* Клавиши */}
                  <div className="card space-y-2.5">
                    <div className="flex items-center gap-2 font-semibold text-sm">
                      <Keyboard className="w-4 h-4 text-accent" /> Горячие клавиши
                      <span className="ml-auto text-[11px] text-text-secondary font-normal">работают даже в игре</span>
                    </div>
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-text-secondary">Показать/скрыть панель</span>
                      <HotkeyCapture value={s.overlayHotkey} onChange={(v) => s.patch({ overlayHotkey: v })} />
                    </div>
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-text-secondary">Перевести сейчас</span>
                      <HotkeyCapture value={s.translateHotkey} onChange={(v) => s.patch({ translateHotkey: v })} />
                    </div>
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-text-secondary">Выделить область и перевести</span>
                      <HotkeyCapture value={s.captureHotkey} onChange={(v) => s.patch({ captureHotkey: v })} />
                    </div>
                    <button
                      onClick={() => emit('trc:translate-now')}
                      className="w-full py-2 rounded-lg border border-white/10 text-xs text-text-secondary hover:border-accent/50 hover:text-white transition-colors"
                    >
                      Проверить «перевести сейчас»
                    </button>
                  </div>

                  {/* Приложение */}
                  <div className="card flex items-center gap-2">
                    <BellRing className="w-4 h-4 text-accent shrink-0" />
                    <div className="text-[11px] text-text-secondary leading-relaxed">
                      Крестик сворачивает в трей — панель и хоткеи продолжают работать.
                    </div>
                    <div className="ml-auto flex gap-2 shrink-0">
                      <button onClick={() => api.hideToTray()} title="В трей" className="p-2 rounded-lg hover:bg-white/10"><Minimize2 className="w-4 h-4 text-text-secondary" /></button>
                      <button onClick={() => api.quitApp()} title="Выйти полностью" className="p-2 rounded-lg hover:bg-danger/20"><XCircle className="w-4 h-4 text-text-secondary" /></button>
                    </div>
                  </div>
                  <div className="h-2" />
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>
    </div>
  );
}
