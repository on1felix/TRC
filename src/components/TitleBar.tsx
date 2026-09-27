import { Minus, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api } from '../api';

export function TitleBar() {
  const appWindow = getCurrentWindow();

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
        <img src="/app-icon.png" alt="" className="w-5 h-5 rounded" />
        <span className="text-sm font-medium text-text-2">
          TR<span className="text-accent">C</span>
          <span className="ml-2 text-[11px] text-text-secondary font-normal">живой перевод экрана · EN→RU</span>
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
