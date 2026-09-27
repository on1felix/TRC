import { useCallback, useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { ArrowLeftRight, Copy, Check, Eraser, Loader2, Languages, ClipboardPaste } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { writeText as writeClipboard, readText as readClipboard } from '@tauri-apps/plugin-clipboard-manager';
import { api, langName, type CaptureResult, type TrcSettings } from '../api';
import { useTrc } from '../store';
import { Toggle } from './Toggle';

export function TranslatorCard({ compact = false }: { compact?: boolean }) {
  const s = useTrc();
  const [input, setInput] = useState('');
  const [output, setOutput] = useState('');
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [err, setErr] = useState('');
  const [via, setVia] = useState<{ engine: string; fallback: boolean } | null>(null);
  const seq = useRef(0);
  const inputRef = useRef(input);
  inputRef.current = input;
  const inputEl = useRef<HTMLTextAreaElement>(null);
  const outputEl = useRef<HTMLTextAreaElement>(null);

  // Ручные Ctrl+C/X/V через системный буфер (в WebView встроенные режутся правами).
  const onFieldKeys = (which: 'in' | 'out') => async (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      void doTranslate();
      return;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    // учитываем русскую раскладку: с=C, ч=X, в=V на тех же клавишах
    const k = e.key.toLowerCase();
    const isC = k === 'c' || k === 'с';
    const isX = k === 'x' || k === 'ч';
    const isV = k === 'v' || k === 'в';
    const el = which === 'in' ? inputEl.current : outputEl.current;
    if (isC) {
      const sel = el && el.selectionStart !== el.selectionEnd
        ? el.value.slice(el.selectionStart, el.selectionEnd)
        : el?.value ?? '';
      if (!sel) return;
      e.preventDefault();
      try {
        await writeClipboard(sel);
      } catch {
        try {
          await navigator.clipboard.writeText(sel);
        } catch {
          /* ignore */
        }
      }
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } else if (isX && which === 'in' && el) {
      if (el.selectionStart === el.selectionEnd) return;
      e.preventDefault();
      const sel = el.value.slice(el.selectionStart, el.selectionEnd);
      try {
        await writeClipboard(sel);
      } catch {
        /* ignore */
      }
      const v = el.value;
      setInput(v.slice(0, el.selectionStart) + v.slice(el.selectionEnd));
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } else if (isV && which === 'in' && el) {
      e.preventDefault();
      let t = '';
      try {
        t = await readClipboard();
      } catch {
        try {
          t = await navigator.clipboard.readText();
        } catch {
          return;
        }
      }
      if (!t) return;
      const v = el.value;
      const pos = el.selectionStart;
      setInput(v.slice(0, el.selectionStart) + t + v.slice(el.selectionEnd));
      requestAnimationFrame(() => {
        el.focus();
        el.setSelectionRange(pos + t.length, pos + t.length);
      });
    }
  };

  const doTranslate = useCallback(async (text?: string) => {
    const t = (text ?? inputRef.current).trim();
    if (!t) {
      setOutput('');
      setErr('');
      return;
    }
    const my = ++seq.current;
    setBusy(true);
    setErr('');
    try {
      // Настройки берём свежие из бэкенда каждый раз —
      // тогда движок из главного окна гарантированно действует и в панели.
      const st = await api.getSettings().catch(() => useTrc.getState());
      useTrc.getState().applyRemote(st);
      const r = await api.translateText(t, st.engine, st.libreUrl, st.source, st.target);
      if (seq.current === my) setOutput(r);
    } catch (e) {
      if (seq.current === my) setErr(`Не перевелось: ${String(e)}`);
    } finally {
      if (seq.current === my) setBusy(false);
    }
  }, []);

  // живой перевод как печатаешь (дебаунс 500мс — без лагов)
  useEffect(() => {
    if (!s.loaded || !s.live) return;
    if (!input.trim()) {
      setOutput('');
      return;
    }
    const id = setTimeout(() => void doTranslate(), 500);
    return () => clearTimeout(id);
  }, [input, s.live, s.source, s.target, s.engine, s.loaded, doTranslate]);

  // хоткей «перевести сейчас» из трея/глобального хоткея
  useEffect(() => {
    let off: (() => void) | undefined;
    listen('trc:translate-now', () => void doTranslate()).then((f) => {
      off = f;
    }).catch(() => {});
    return () => off?.();
  }, [doTranslate]);

  // синхронизация настроек между главным окном и панелью
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<TrcSettings>('trc:settings-updated', (e) => useTrc.getState().applyRemote(e.payload)).then((f) => {
      off = f;
    }).catch(() => {});
    return () => off?.();
  }, []);

  // результат выделения области: оригинал в ввод, перевод в вывод
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<CaptureResult>('trc:capture-result', (e) => {
      seq.current++;
      setInput(e.payload.original);
      setOutput(e.payload.translated);
      setErr('');
    }).then((f) => {
      off = f;
    }).catch(() => {});
    return () => off?.();
  }, []);

  // каким движком реально переведено (видно срабатывание страховки)
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<{ engine: string; fallback: boolean }>('trc:engine-used', (e) => {
      setVia(e.payload);
    }).then((f) => {
      off = f;
    }).catch(() => {});
    return () => off?.();
  }, []);

  const ENGINE_NAMES: Record<string, string> = {
    yandex: 'Яндекс',
    bing: 'Bing',
    google: 'Google',
    mymemory: 'MyMemory',
    libre: 'LibreTranslate',
  };

  const swap = () => {
    const st = useTrc.getState();
    void s.patch({ source: st.target, target: st.source });
    setInput(output);
    setOutput(input);
  };

  const copy = async () => {
    if (!output) return;
    // 1) системный буфер через Tauri-плагин (работает всегда),
    // 2) Web API, 3) старый execCommand
    try {
      await writeClipboard(output);
    } catch {
      try {
        await navigator.clipboard.writeText(output);
      } catch {
        const ta = document.createElement('textarea');
        ta.value = output;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        ta.remove();
      }
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  const paste = async () => {
    try {
      const t = await readClipboard();
      if (t) setInput(t);
      return;
    } catch {
      /* fallthrough */
    }
    try {
      const t = await navigator.clipboard.readText();
      if (t) setInput(t);
    } catch {
      /* буфер недоступен — вставь через Ctrl+V */
    }
  };

  return (
    <div className={compact ? 'space-y-2.5' : 'card gradient-border space-y-3'}>
      {/* направление */}
      <div className="flex items-center gap-2">
        <Languages className="w-4 h-4 text-accent shrink-0" />
        <button
          onClick={() => {
            const opts: string[] = ['en', 'ru'];
            const next = opts[(opts.indexOf(s.source) + 1) % opts.length];
            void s.patch({ source: next, target: s.target === next ? s.source : s.target });
          }}
          className="px-3 py-1.5 rounded-lg border border-border bg-white/[0.04] text-xs font-semibold hover:border-accent/60 transition-colors"
        >
          {langName(s.source)}
        </button>
        <button onClick={swap} title="Поменять местами" className="p-1.5 rounded-lg hover:bg-white/10 transition-colors">
          <ArrowLeftRight className="w-4 h-4 text-accent" />
        </button>
        <button
          onClick={() => {
            const opts: string[] = ['en', 'ru'];
            const next = opts[(opts.indexOf(s.target) + 1) % opts.length];
            void s.patch({ target: next, source: s.source === next ? s.target : s.source });
          }}
          className="px-3 py-1.5 rounded-lg border border-border bg-white/[0.04] text-xs font-semibold hover:border-accent/60 transition-colors"
        >
          {langName(s.target)}
        </button>
        {!compact && (
          <label className="ml-auto flex items-center gap-2 text-[11px] text-text-secondary cursor-pointer">
            <Toggle checked={s.live} onChange={(v) => s.patch({ live: v })} />
            Как печатаешь
          </label>
        )}
      </div>

      {/* ввод — текст можно писать, вставлять и выделять */}
      <textarea
        ref={inputEl}
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={onFieldKeys('in')}
        placeholder={s.source === 'en' ? 'Type or paste English text…' : 'Пиши или вставь текст…'}
        spellCheck={false}
        className={`w-full rounded-lg bg-white/[0.04] border border-border focus:border-accent/60 transition-colors text-sm p-3 resize-none select-text ${compact ? 'h-28' : 'h-32'}`}
        style={{ userSelect: 'text', WebkitUserSelect: 'text' }}
      />

      <div className="flex items-center gap-2">
        <motion.button
          whileTap={{ scale: 0.97 }}
          onClick={() => doTranslate()}
          disabled={busy || !input.trim()}
          className="flex items-center gap-2 px-4 py-2 rounded-lg bg-accent text-black text-xs font-bold hover:bg-accent-light transition-colors disabled:opacity-40"
        >
          {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : null}
          Перевести
        </motion.button>
        <button onClick={copy} disabled={!output} title="Скопировать перевод" className="flex items-center gap-1.5 px-3 py-2 rounded-lg border border-white/15 text-xs text-text-secondary hover:border-accent/60 hover:text-white transition-colors disabled:opacity-40">
          {copied ? <Check className="w-3.5 h-3.5 text-success" /> : <Copy className="w-3.5 h-3.5" />}
          {copied ? 'Скопировано' : 'Копия'}
        </button>
        <button onClick={paste} title="Вставить из буфера" className="flex items-center gap-1.5 px-3 py-2 rounded-lg border border-white/15 text-xs text-text-secondary hover:border-accent/60 hover:text-white transition-colors">
          <ClipboardPaste className="w-3.5 h-3.5" />
          Вставить
        </button>
        <button onClick={() => { setInput(''); setOutput(''); setErr(''); }} title="Очистить" className="p-2 rounded-lg hover:bg-white/10 transition-colors">
          <Eraser className="w-4 h-4 text-text-secondary" />
        </button>
        <span className="ml-auto text-[10px] text-text-muted font-mono">
          {input.length} · Ctrl+Enter
        </span>
      </div>

      {/* результат — можно выделять и копировать вручную */}
      <textarea
        ref={outputEl}
        value={output}
        readOnly
        onKeyDown={onFieldKeys('out')}
        placeholder="Перевод появится здесь…"
        spellCheck={false}
        className={`w-full rounded-lg bg-accent/[0.06] border border-accent/20 text-sm p-3 resize-none select-text ${compact ? 'h-28' : 'h-32'}`}
        style={{ userSelect: 'text', WebkitUserSelect: 'text' }}
      />
      {err && <div className="text-[11px] text-danger">{err}</div>}
      {via && output && !err && (
        <div className="text-[10px] text-text-muted">
          через {ENGINE_NAMES[via.engine] ?? via.engine}
          {via.fallback ? ' · страховка (выбранный движок отдыхает)' : ''}
        </div>
      )}
    </div>
  );
}
