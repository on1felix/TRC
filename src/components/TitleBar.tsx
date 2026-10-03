import { useEffect, useState } from 'react';
import { Minus, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getVersion } from '@tauri-apps/api/app';
import { api } from '../api';

export function TitleBar() {
  const appWindow = getCurrentWindow();
  const [version, setVersion] = useState('');

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    e.preventDefault();
    void appWindow.startDragging();
  };

  return (
    <div
      onMouseDown={handleMouseDown}
      className="h-10 flex items-center justify-between px-4 bg-bg-2 border-b border-border select-none shrink-0"
    >
      <div className="flex items-center gap-2 pointer-events-none">
        <img src="/app-icon.svg" alt="" className="w-5 h-5 rounded" />
        <span className="text-sm font-medium text-text-2">
          TR<span className="text-accent">C</span>
          {version && (
            <span className="ml-1.5 text-[10px] font-semibold text-text-muted/70 px-1.5 py-[1px] rounded border border-border bg-white/[0.03]">
              v{version}
            </span>
          )}
        </span>
      </div>
      <div className="flex items-center gap-1">
        <button
          onClick={() => api.hideToTray()}
          title="Свернуть в трей"
          className="w-8 h-8 flex items-center justify-center rounded hover:bg-white/10 transition-colors"
        >
          <Minus className="w-4 h-4 text-text-secondary" />
        </button>
        <button
          onClick={() => api.hideToTray()}
          title="Закрыть в трей (перевод продолжит работать)"
          className="group w-8 h-8 flex items-center justify-center rounded hover:bg-danger/30 transition-colors"
        >
          <X className="w-4 h-4 text-text-secondary transition-colors group-hover:text-[#ff6666]" />
        </button>
      </div>
    </div>
  );
}
