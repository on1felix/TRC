import { useEffect, useState } from 'react';
import { X, ScanSearch, Settings as SettingsIcon } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, ENGINES } from '../api';
import { useTrc } from '../store';
import { AppBg } from '../components/AppBg';
import { Toggle } from '../components/Toggle';
import { TranslatorCard } from '../components/TranslatorCard';

// Компактная панель переводчика поверх игр.
// Окно всегда сверху (alwaysOnTop в Rust), таскается за шапку.
export function PanelOverlay() {
  const s = useTrc();
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    void s.load();
    // Панель открывают хоткеем — при каждом фокусе подтягиваем свежие
    // настройки (движок, направление), чтобы всё из главного окна действовало.
    let off: (() => void) | undefined;
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) void useTrc.getState().load();
      })
      .then((f) => {
        off = f;
      })
      .catch(() => {});
    return () => off?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const drag = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('button') || target.closest('textarea') || target.closest('input')) return;
    e.preventDefault();
    void getCurrentWindow().startDragging();
  };

  if (!s.loaded) {
    return (
      <div className="h-screen bg-bg flex items-center justify-center text-xs text-text-secondary relative">
        <AppBg />
        <span className="relative z-10">Загрузка…</span>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-bg overflow-hidden relative rounded-xl">
      <AppBg />
      <div className="relative z-10 flex flex-col h-full">
      <div
        onMouseDown={drag}
        className="flex items-center gap-2 px-3 h-9 border-b border-white/10 select-none shrink-0 cursor-move"
      >
        <img src="/app-icon.png" alt="" className="w-4 h-4 rounded pointer-events-none" />
        <span className="text-xs font-semibold pointer-events-none">
          TR<span className="text-accent">C</span>
          <span className="ml-1.5 font-normal text-text-secondary">· {s.overlayHotkey} — скрыть</span>
        </span>
        <button
          onClick={() => api.prepareSelection().catch(() => {})}
          title={`Выделить область (${s.captureHotkey})`}
          className="ml-auto w-7 h-7 flex items-center justify-center rounded hover:bg-white/10"
        >
          <ScanSearch className="w-3.5 h-3.5 text-text-secondary" />
        </button>
        <button
          onClick={() => setShowSettings((v) => !v)}
          title="Настройки панели"
          className={`w-7 h-7 flex items-center justify-center rounded transition-colors ${showSettings ? 'bg-accent/20' : 'hover:bg-white/10'}`}
        >
          <SettingsIcon className={`w-3.5 h-3.5 ${showSettings ? 'text-accent' : 'text-text-secondary'}`} />
        </button>
        <button
          onClick={() => getCurrentWindow().hide().catch(() => api.hideToTray())}
          title="Скрыть панель"
          className="w-7 h-7 flex items-center justify-center rounded hover:bg-danger/30 group"
        >
          <X className="w-3.5 h-3.5 text-text-secondary group-hover:text-[#ff6666]" />
        </button>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-2">
        {showSettings && (
          <div className="p-3 rounded-xl border border-white/10 bg-white/[0.04] space-y-2">
            <div className="text-[11px] font-semibold text-text-secondary">Движок перевода</div>
            <div className="grid grid-cols-2 gap-1.5">
              {ENGINES.map((e) => (
                <button
                  key={e.id}
                  onClick={() => s.patch({ engine: e.id })}
                  className={`px-2 py-1.5 rounded-lg border text-[11px] font-medium transition-colors ${
                    s.engine === e.id
                      ? 'border-accent/70 bg-accent/10 text-white'
                      : 'border-white/10 bg-white/[0.03] text-text-secondary hover:border-accent/40'
                  }`}
                >
                  {e.name}
                </button>
              ))}
            </div>
            {s.engine === 'libre' && (
              <input
                value={s.libreUrl}
                onChange={(e) => s.patch({ libreUrl: e.target.value })}
                placeholder="https://libretranslate.de"
                className="w-full px-2 py-1.5 rounded-md bg-white/5 border border-white/10 text-[11px]"
              />
            )}
            <div className="flex items-center justify-between text-[11px] pt-0.5">
              <span className="text-text-secondary">Переводить сразу, как печатаешь</span>
              <Toggle checked={s.live} onChange={(v) => s.patch({ live: v })} />
            </div>
          </div>
        )}
        <TranslatorCard compact />
      </div>
      </div>
    </div>
  );
}
