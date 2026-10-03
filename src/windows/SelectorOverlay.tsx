import { useCallback, useEffect, useRef, useState } from 'react';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Loader2, AlertTriangle } from 'lucide-react';
import { api, detectLang, type CaptureResult } from '../api';
import { ocrDataUrl, cropDataUrl, cleanOcrText, wordSanity } from '../lib/ocr';

type Phase = 'shot' | 'select' | 'work' | 'empty' | 'error';
interface Rect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

// Выбор области. Кадр уже снят ДО показа окна (prepare_selection),
// здесь только берём его, показываем вписанным (contain) и даём рамку.
// Esc / ПКМ — отмена.
export function SelectorOverlay() {
  const [shot, setShot] = useState<{ url: string; w: number; h: number } | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [drag, setDrag] = useState<{ x: number; y: number } | null>(null);
  const [phase, setPhase] = useState<Phase>('shot');
  const [status, setStatus] = useState('Готовлю кадр…');
  const busy = useRef(false);
  const runId = useRef(0);

  // Математика contain-раскладки: кадр вписан в окно, поля по бокам пустые.
  const layout = useCallback(() => {
    if (!shot) return null;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const s = Math.min(vw / shot.w, vh / shot.h);
    const dw = shot.w * s;
    const dh = shot.h * s;
    return { s, ox: (vw - dw) / 2, oy: (vh - dh) / 2 };
  }, [shot]);

  // Каждый показ окна — чистый старт с готовым кадром.
  // Ждём видимости и кадра ретраями: фокус иногда приходит раньше,
  // чем окно реально показано — отсюда был «серый экран».
  const start = useCallback(async () => {
    const my = ++runId.current;
    busy.current = false;
    setRect(null);
    setDrag(null);
    setShot(null);
    setPhase('shot');
    setStatus('Беру кадр…');
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    // 1. ждём, пока окно реально видимо (до 5 сек)
    let visible = false;
    for (let i = 0; i < 25; i++) {
      if (runId.current !== my) return;
      try {
        if (await getCurrentWindow().isVisible()) {
          visible = true;
          break;
        }
      } catch {
        /* ignore */
      }
      await sleep(200);
    }
    if (!visible || runId.current !== my) return; // скрыто — ждём следующего показа
    // 2. забираем кадр (до 3 сек ретраев)
    for (let i = 0; i < 10; i++) {
      if (runId.current !== my) return;
      try {
        const c = await api.takeSelectionImage();
        if (runId.current !== my) return;
        setShot({ url: `data:image/png;base64,${c.pngBase64}`, w: c.w, h: c.h });
        setPhase('select');
        return;
      } catch {
        await sleep(300);
      }
    }
    if (runId.current !== my) return;
    setPhase('error');
    setStatus('Нет кадра — закрой (Esc) и нажми F9 ещё раз');
  }, []);

  const startRef = useRef(start);
  startRef.current = start;

  useEffect(() => {
    void start();
    // Явный сигнал от бэкенда после show — главный триггер старта.
    let offOpen: (() => void) | undefined;
    listen('trc:select-opened', () => {
      if (!busy.current) void startRef.current();
    }).then((f) => {
      offOpen = f;
    }).catch(() => {});
    // Фокус — запасной триггер (окно не размонтируется при скрытии).
    let off: (() => void) | undefined;
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused && !busy.current) void startRef.current();
      })
      .then((f) => {
        off = f;
      })
      .catch(() => {});
    const h = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        runId.current++;
        busy.current = false;
        void api.closeSelector();
      }
    };
    window.addEventListener('keydown', h);
    return () => {
      runId.current++;
      window.removeEventListener('keydown', h);
      offOpen?.();
      off?.();
    };
  }, [start]);

  const cancel = () => {
    runId.current++;
    busy.current = false;
    void api.closeSelector();
  };

  const finish = async (r: Rect) => {
    const L = layout();
    if (busy.current || !shot || !L) return;
    // клиентские координаты → пиксели кадра
    const x = Math.round((Math.min(r.x0, r.x1) - L.ox) / L.s);
    const y = Math.round((Math.min(r.y0, r.y1) - L.oy) / L.s);
    const w = Math.round(Math.abs(r.x1 - r.x0) / L.s);
    const h = Math.round(Math.abs(r.y1 - r.y0) / L.s);
    // Мелкое выделение тоже принимаем (от 3px), OCR его растянет.
    if (w < 3 || h < 3) {
      setRect(null);
      return;
    }
    const cx = Math.max(0, Math.min(shot.w - 1, x));
    const cy = Math.max(0, Math.min(shot.h - 1, y));
    const cw = Math.max(1, Math.min(shot.w - cx, w));
    const ch = Math.max(1, Math.min(shot.h - cy, h));
    busy.current = true;
    const my = runId.current;
    const alive = () => runId.current === my;
    let engineName = 'system';
    setPhase('work');
    try {
      setStatus('Вырезаю…');
      const crop = await cropDataUrl(shot.url, cx, cy, cw, ch);
      if (!alive()) return;
      const st = await api.getSettings();
      engineName = st.ocrEngine;
      if (!alive()) return;
      setStatus('Распознаю текст…');
      // Движок из настроек: system (Windows OCR) или builtin (tesseract).
      const useBuiltin = st.ocrEngine === 'builtin';
      if (useBuiltin) setStatus('Распознаю текст (встроенный)…');
      const readAs = async (lang: string) =>
        useBuiltin
          ? ocrDataUrl(crop, lang)
          : api.ocrImage(crop.split(',')[1] ?? '', lang);
      let original = cleanOcrText(await readAs(st.source));
      if (!alive()) return;
      // Чужим движком выходит транслит-мусор вида «npoAaM» — тогда читаем
      // вторым языком и берём более вменяемый вариант.
      const otherLang = st.source === 'ru' ? 'en' : 'ru';
      if (wordSanity(original) < 0.7) {
        setStatus('Перечитываю другим языком…');
        const retry = cleanOcrText(await readAs(otherLang));
        if (!alive()) return;
        if (wordSanity(retry) > wordSanity(original)) original = retry;
      }
      if (!alive()) return;
      if (!original || original.replace(/[^a-zA-Zа-яА-ЯёЁ]/g, '').length < 2) {
        setPhase('empty');
        setStatus('Текст не найден — выдели заново или Esc');
        setRect(null);
        setPhase('select');
        return;
      }
      // Язык распознанного — ведущий: направление переключается само.
      let source = st.source;
      let target = st.target;
      const detected = detectLang(original);
      if (detected && detected !== source) {
        source = detected;
        target = detected === 'ru' ? 'en' : 'ru';
        await api.saveSettings({ ...st, source, target }).catch(() => {});
        if (!alive()) return;
      }
      setStatus('Перевожу…');
      const translated = await api.translateText(original, st.engine, st.libreUrl, source, target);
      if (!alive()) return;
      const res: CaptureResult = { original, translated };
      await emit('trc:capture-result', res);
      // Успех: окна НЕ возвращаем — откроется только панель.
      await api.closeSelector(false);
      await api.showPanel();
    } catch (e) {
      if (!alive()) return;
      // Ошибку показываем и НЕ закрываем — видно, что случилось.
      // Подсказываем второй способ: выбор стрелкой у статуса OCR в главном окне.
      const other = engineName === 'builtin' ? '«Системный»' : '«Встроенный»';
      setPhase('error');
      setStatus(
        `Не вышло: ${e instanceof Error ? e.message : String(e)} — выдели заново, Esc, или попробуй движок ${other}`,
      );
    } finally {
      if (alive()) busy.current = false;
    }
  };

  const picking = phase === 'select' || phase === 'error';
  const rw = rect ? Math.abs(rect.x1 - rect.x0) : 0;
  const rh = rect ? Math.abs(rect.y1 - rect.y0) : 0;

  return (
    <div
      className="fixed inset-0 bg-black overflow-hidden select-none flex items-center justify-center"
      style={{ cursor: picking ? 'crosshair' : 'default' }}
      onMouseDown={(e) => {
        if (!picking || e.button !== 0) return;
        if (phase === 'error') setPhase('select');
        setDrag({ x: e.clientX, y: e.clientY });
        setRect({ x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY });
      }}
      onMouseMove={(e) => {
        if (!picking || !drag) return;
        setRect({ x0: drag.x, y0: drag.y, x1: e.clientX, y1: e.clientY });
      }}
      onMouseUp={() => {
        if (!picking || !rect) return;
        setDrag(null);
        void finish(rect);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        cancel();
      }}
    >
      {shot && (
        <img
          src={shot.url}
          alt=""
          draggable={false}
          className="max-w-full max-h-full"
          style={{ filter: 'brightness(0.45)', pointerEvents: 'none', objectFit: 'contain' }}
        />
      )}
      {/* рамка с живым размером */}
      {rect && picking && (
        <div
          className="absolute rounded border-2 border-accent shadow-[0_0_24px_rgba(94,143,194,0.5)] pointer-events-none"
          style={{
            left: Math.min(rect.x0, rect.x1),
            top: Math.min(rect.y0, rect.y1),
            width: rw,
            height: rh,
            background: 'rgba(94,143,194,0.08)',
          }}
        >
          <div className="absolute -bottom-6 left-0 text-[11px] px-2 py-0.5 rounded bg-black/80 text-accent font-mono whitespace-nowrap">
            {Math.round(rw)}×{Math.round(rh)}
          </div>
        </div>
      )}
      {/* подсказка / статус */}
      <div className="absolute top-4 left-1/2 -translate-x-1/2 flex items-center gap-2 px-4 py-2.5 rounded-2xl bg-[#14171d] border border-white/10 shadow-2xl text-xs text-text-secondary whitespace-nowrap max-w-[90vw]">
        {(phase === 'shot' || phase === 'work') && <Loader2 className="w-4 h-4 text-accent animate-spin shrink-0" />}
        {phase === 'error' && <AlertTriangle className="w-4 h-4 text-danger shrink-0" />}
        {picking ? (
          <span className="truncate">
            Тяни рамку по тексту · <b className="text-white">ПКМ/Esc</b> — отмена
          </span>
        ) : (
          <span className="truncate">{status}</span>
        )}
      </div>
    </div>
  );
}
