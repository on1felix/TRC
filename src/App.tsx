import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { MainPage } from './pages/MainPage';
import { PanelOverlay } from './windows/PanelOverlay';
import { SelectorOverlay } from './windows/SelectorOverlay';

export default function App() {
  const [label, setLabel] = useState('main');

  useEffect(() => {
    setLabel(getCurrentWindow().label);
    // Блокируем системное меню везде, КРОМЕ полей ввода —
    // там нужно родное меню (Копировать/Вставить/Выделить).
    const handler = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest('textarea, input')) return;
      e.preventDefault();
    };
    document.addEventListener('contextmenu', handler);
    return () => document.removeEventListener('contextmenu', handler);
  }, []);

  if (label === 'panel') return <PanelOverlay />;
  if (label === 'select') return <SelectorOverlay />;
  return <MainPage />;
}
