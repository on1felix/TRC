import { useEffect, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Languages, Keyboard, BellRing, Minimize2, XCircle, PanelTop, ScanSearch, Settings as SettingsIcon, ArrowLeft, ChevronDown } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { api, ENGINES, type OcrEngineId, type UpdateInfo, type UpdateProgress } from '../api';
import { useTrc } from '../store';
import { TitleBar } from '../components/TitleBar';
import { AppBg } from '../components/AppBg';
import { Toggle } from '../components/Toggle';
import { HotkeyCapture } from '../components/HotkeyCapture';
import { TranslatorCard } from '../components/TranslatorCard';
import { UpdateModal } from '../components/UpdateModal';
import { reregisterHotkeys } from '../lib/hotkeys';

export function MainPage() {
  const s = useTrc();
  const [tab, setTab] = useState<'home' | 'settings'>('home');
  const [ocrState, setOcrState] = useState<{ checked: boolean; error: string }>({
    checked: false,
    error: '',
  });
  const [ocrMenu, setOcrMenu] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [restarting, setRestarting] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const applyingRef = useRef(false);

  // Автообновление с GitHub-релизов: проверка при старте → скачивание → замена.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<UpdateProgress>('update-progress', (e) => setUpdateProgress(e.payload)).then((f) => {
      unlisten = f;
    }).catch(() => {});

    const timer = setTimeout(async () => {
      try {
        const info = await api.checkUpdate();
        if (info) {
          setUpdateInfo(info);
          await api.downloadUpdate(info.download_url);
        }
      } catch (e) {
        setUpdateError(String(e));
      }
    }, 2000);

    return () => {
      clearTimeout(timer);
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (updateProgress && updateProgress.percent >= 100 && !applyingRef.current) {
      applyingRef.current = true;
      setRestarting(true);
      setTimeout(() => {
        api.applyUpdate().catch((e) => setUpdateError(String(e)));
      }, 900);
    }
  }, [updateProgress]);

  useEffect(() => {
    // Реальная проверка системного движка: готов — «OCR готов»,
    // нет — причина из бэкенда.
    api
      .ocrAvailable()
      .then(() => setOcrState({ checked: true, error: '' }))
      .catch((e) => setOcrState({ checked: true, error: String(e) }));
  }, []);

  useEffect(() => {
    void s.load().then(() => {
      const st = useTrc.getState();
      void reregisterHotkeys(st.overlayHotkey, st.captureHotkey);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!s.loaded) return;
    void reregisterHotkeys(s.overlayHotkey, s.captureHotkey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.overlayHotkey, s.captureHotkey]);

  // Кнопки мыши (обрабатывает нативный хук в Rust).
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<string>('trc:mouse-hotkey', (e) => {
      if (e.payload === 'overlay') void api.togglePanel();
      else if (e.payload === 'capture') void api.prepareSelection().catch(() => {});
    }).then((f) => {
      off = f;
    }).catch(() => {});
    return () => off?.();
  }, []);

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

  // Выбор движка F9: system (как в «Ножницах», по умолчанию) или builtin (tesseract).
  const pickOcr = (v: OcrEngineId) => {
    setOcrMenu(false);
    if (v !== useTrc.getState().ocrEngine) void s.patch({ ocrEngine: v });
  };

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
                  <div className="relative flex items-center justify-center gap-1.5 text-[11px] text-text-muted text-center">
                    <button
                      onClick={() => setOcrMenu((v) => !v)}
                      title="Выбрать движок распознавания (F9)"
                      className="flex items-center gap-1 px-2 py-1 rounded-lg border border-white/10 bg-white/[0.04] hover:border-accent/60 transition-colors"
                    >
                      <span className="font-semibold text-text-secondary">
                        OCR: {s.ocrEngine === 'builtin' ? 'Встроенный' : 'Системный'}
                      </span>
                      <ChevronDown
                        className={`w-3.5 h-3.5 text-accent transition-transform duration-200 ${ocrMenu ? 'rotate-180' : ''}`}
                      />
                    </button>
                    {ocrMenu && (
                      <div className="absolute bottom-full mb-1.5 left-1/2 -translate-x-1/2 w-64 rounded-xl border border-white/10 bg-[#14171d] shadow-2xl p-1 z-20 text-left">
                        {(
                          [
                            { id: 'system', name: 'Системный', desc: 'по умолчанию' },
                            { id: 'builtin', name: 'Встроенный', desc: 'tesseract · на случай проблем' },
                          ] as const
                        ).map((o) => (
                          <button
                            key={o.id}
                            onClick={() => pickOcr(o.id)}
                            className={`w-full px-2.5 py-1.5 rounded-lg transition-colors text-left ${
                              s.ocrEngine === o.id ? 'bg-accent/15' : 'hover:bg-white/5'
                            }`}
                          >
                            <span className={`block text-xs font-semibold ${s.ocrEngine === o.id ? 'text-white' : 'text-text-secondary'}`}>
                              {o.name}
                            </span>
                            <span className="block text-[10px] text-text-muted">{o.desc}</span>
                          </button>
                        ))}
                      </div>
                    )}
                    {s.ocrEngine === 'builtin' ? (
                      <span className="text-accent">Встроенный движок</span>
                    ) : !ocrState.checked ? (
                      <span>Проверяю распознавание…</span>
                    ) : !ocrState.error ? (
                      <span className="text-success">OCR готов</span>
                    ) : (
                      <span className="text-danger">OCR: {ocrState.error}</span>
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
                    </div>                    <div className="flex items-center justify-between text-xs">
                      <span className="text-text-secondary">Показать/скрыть панель</span>
                      <HotkeyCapture value={s.overlayHotkey} onChange={(v) => s.patch({ overlayHotkey: v })} />
                    </div>
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-text-secondary">Выделить область и перевести</span>
                      <HotkeyCapture value={s.captureHotkey} onChange={(v) => s.patch({ captureHotkey: v })} />
                    </div>
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

        {/* Окно автообновления */}
        <AnimatePresence>
          {updateInfo && (
            <UpdateModal
              info={updateInfo}
              progress={updateProgress}
              restarting={restarting}
              error={updateError}
            />
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
