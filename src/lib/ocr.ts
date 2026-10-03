// Встроенный OCR (tesseract.js, eng+rus) — второй движок F9 («Встроенный»).
// Первый запуск докачивает движок и traineddata, дальше всё офлайн (кэш в IDB).

import { createWorker, PSM, type Worker } from 'tesseract.js';

export type OcrStage = 'idle' | 'loading' | 'ready' | 'error';

const workers = new Map<string, Promise<Worker>>();
let stage: OcrStage = 'idle';
let stageDetail = '';
const subs = new Set<(s: OcrStage, detail: string, progress: number) => void>();
let lastProgress = 0;

function publish(progress = lastProgress) {
  lastProgress = progress;
  subs.forEach((fn) => fn(stage, stageDetail, progress));
}

export function onOcrStage(fn: (s: OcrStage, detail: string, progress: number) => void) {
  subs.add(fn);
  fn(stage, stageDetail, lastProgress);
  return () => {
    subs.delete(fn);
  };
}

// Воркер под язык исходника: eng-only для английского читает чище,
// чем eng+rus (нет кириллических confusion вида n!code → п!сойе — проверено A/B).
function langsFor(lang: string): string[] {
  if (lang === 'ru') return ['rus'];
  if (lang === 'en') return ['eng'];
  return ['eng', 'rus'];
}

export function ensureOcrFor(lang: string): Promise<Worker> {
  const key = langsFor(lang).join('+');
  let p = workers.get(key);
  if (!p) {
    stage = 'loading';
    stageDetail = 'Загружаю движок OCR…';
    publish(0);
    p = (async () => {
      try {
        const w = await createWorker(langsFor(lang), undefined, {
          logger: (m) => {
            if (m.status === 'recognizing text') {
              stageDetail = `Распознаю… ${Math.round((m.progress || 0) * 100)}%`;
              publish(m.progress || 0);
            } else {
              // loading tesseract core / language traineddata / initializing api
              stageDetail = ocrStatusText(m.status);
              publish(0);
            }
          },
        });
        // SINGLE_BLOCK: для вырезанных кусков точнее AUTO (проверено A/B-тестом).
        // blacklist '|': движок путает I и | — запрещаем ему черту вовсе,
        // тогда I читается правильно (проверено A/B-тестом на Discord-образце).
        await w.setParameters({
          tessedit_pageseg_mode: PSM.SINGLE_BLOCK,
          tessedit_char_blacklist: '|',
        });
        stage = 'ready';
        stageDetail = 'OCR готов';
        publish(1);
        return w;
      } catch (e) {
        // сбрасываем, чтобы следующая попытка попробовала заново
        workers.delete(key);
        stage = 'error';
        stageDetail = `OCR не загрузился: ${e instanceof Error ? e.message : String(e)}`;
        publish(0);
        throw e;
      }
    })();
    workers.set(key, p);
  }
  return p;
}

export function ensureOcr(): Promise<Worker> {
  return ensureOcrFor('en');
}

function ocrStatusText(status: string): string {
  if (status.includes('core')) return 'Качаю ядро OCR…';
  if (status.includes('traineddata') || status.includes('language')) return 'Качаю языковые данные (EN+RU)…';
  if (status.includes('initializing')) return 'Инициализирую OCR…';
  return 'Готовлю OCR…';
}

export function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const id = setTimeout(() => reject(new Error(label)), ms);
    p.then(
      (v) => {
        clearTimeout(id);
        resolve(v);
      },
      (e) => {
        clearTimeout(id);
        reject(e);
      },
    );
  });
}

// Предобработка: мелочь тянем nearest-neighbour на целом множителе
// (пиксельные шрифты вроде майнкрафта так читаются лучше — проверено A/B),
// крупное — мягким апскейлом. Плюс grayscale + мягкий порог (threshold=false —
// без порога, для второго шанса). Stretch/Otsu по A/B ничего не дали — не используем.
export async function preprocess(dataUrl: string, threshold: boolean = true): Promise<string> {
  const img = new Image();
  await new Promise<void>((res, rej) => {
    img.onload = () => res();
    img.onerror = () => rej(new Error('img load'));
    img.src = dataUrl;
  });
  const longSide = Math.max(1, Math.max(img.width, img.height));
  let scale: number;
  let smooth: boolean;
  if (longSide < 240) {
    // целая кратность до ~660px по длинной стороне, без сглаживания
    scale = Math.min(8, Math.max(2, Math.ceil(660 / longSide)));
    smooth = false;
  } else {
    scale = 2;
    smooth = true;
  }
  const canvas = document.createElement('canvas');
  canvas.width = Math.min(2600, Math.round(img.width * scale));
  canvas.height = Math.min(2600, Math.round(img.height * scale));
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.imageSmoothingEnabled = smooth;
  ctx.imageSmoothingQuality = smooth ? 'high' : 'low';
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  if (!threshold) return canvas.toDataURL('image/png');
  const d = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const px = d.data;
  for (let i = 0; i < px.length; i += 4) {
    const g = 0.299 * px[i] + 0.587 * px[i + 1] + 0.114 * px[i + 2];
    const v = g > 160 ? 255 : g < 90 ? 0 : g;
    px[i] = px[i + 1] = px[i + 2] = v;
  }
  ctx.putImageData(d, 0, 0);
  return canvas.toDataURL('image/png');
}

// Чистка текста OCR: сохраняем как скопированное — пробелы, отступы,
// переносы и пустые строки не трогаем. Режем только края и \r.
export function cleanOcrText(t: string): string {
  return (t || '').replace(/\r/g, '').trim();
}

// Вменяемость распознавания 0..1: доля слов с нормальным кейсом
// (строчные / С Заглавной / КАПС). Транслит-мусор чужого движка вида
// «npoAaM» (мешанина кейса внутри слова) даёт низкий скор — повод
// перечитать другим языком.
export function wordSanity(text: string): number {
  const words = (text || '').split(/[^A-Za-zА-Яа-яЁё]+/).filter(Boolean);
  if (!words.length) return 1;
  let ok = 0;
  for (const w of words) {
    if (/^([A-ZА-ЯЁ]?[a-zа-яё]+|[A-ZА-ЯЁ0-9]+)$/.test(w)) ok++;
  }
  return ok / words.length;
}

interface OcrBox {
  t: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

// Слова из TSV tesseract (level 5): left/top/width/height + текст.
function parseTsv(tsv: string): OcrBox[] {
  const out: OcrBox[] = [];
  for (const ln of (tsv || '').split('\n')) {
    const p = ln.split('\t');
    if (p.length < 12 || p[0] !== '5') continue;
    const text = p.slice(11).join('\t').trim();
    if (!text) continue;
    const l = Number(p[6]);
    const t = Number(p[7]);
    const w = Number(p[8]);
    const h = Number(p[9]);
    if (![l, t, w, h].every(Number.isFinite)) continue;
    out.push({ t: text, x0: l, y0: t, x1: l + w, y1: t + h });
  }
  return out;
}

// Та же геометрическая сборка строк, что в бэкенде (rebuild_lines):
// строки по координатам слов, зазоры → пробелы, колонки сохраняются.
function rebuildLines(boxes: OcrBox[]): string {
  const words = boxes.filter((b) => b.t.trim());
  if (!words.length) return '';
  const hs = words.map((w) => Math.max(1, w.y1 - w.y0)).sort((a, b) => a - b);
  const medH = Math.max(4, hs[Math.floor(hs.length / 2)]);
  const charW = Math.max(1, medH * 0.55);
  const byY = [...words].sort((a, b) => a.y0 + a.y1 - (b.y0 + b.y1));
  const rows: OcrBox[][] = [];
  let lastYc = -Infinity;
  for (const w of byY) {
    const yc = (w.y0 + w.y1) / 2;
    if (!rows.length || yc - lastYc > medH * 0.6) rows.push([]);
    lastYc = yc;
    rows[rows.length - 1].push(w);
  }
  for (const r of rows) r.sort((a, b) => a.x0 - b.x0);
  const small: number[] = [];
  for (const r of rows) {
    let px = -Infinity;
    let first = true;
    for (const w of r) {
      if (!first) {
        const gap = w.x0 - px;
        if (gap > 0 && gap < medH) small.push(gap);
      }
      first = false;
      px = w.x1;
    }
  }
  small.sort((a, b) => a - b);
  const spaceUnit = Math.max(1, small.length ? small[Math.floor(small.length / 2)] : charW);
  const out: string[] = [];
  for (const r of rows) {
    let line = '';
    let px = -Infinity;
    for (const w of r) {
      if (line) {
        const n = Math.min(32, Math.max(1, Math.round((w.x0 - px) / spaceUnit)));
        line += ' '.repeat(n);
      }
      line += w.t.trim();
      px = w.x1;
    }
    line = line.trimEnd();
    if (line) out.push(line);
  }
  return out.join('\n').trim();
}

export async function ocrDataUrl(dataUrl: string, lang: string = 'en'): Promise<string> {
  const w = await withTimeout(ensureOcrFor(lang), 120000, 'OCR грузится дольше 2 минут — проверь интернет');
  const pre1 = await preprocess(dataUrl, true);
  // tsv нужен для геометрической сборки строк (слова с координатами).
  const r1 = await withTimeout(
    w.recognize(pre1, {}, { text: true, tsv: true }),
    90000,
    'Распознавание зависло дольше 90 сек',
  );
  const t1 = rebuildLines(parseTsv(r1.data.tsv as unknown as string)) || cleanOcrText(r1.data.text);
  const c1 = r1.data.confidence ?? 0;
  // Уверены — отдаём сразу. Нет — второй шанс без порога
  // (берёт тонкие штрихи и тени, где порог съедает буквы).
  if (c1 >= 82 && t1.replace(/[^a-zA-Zа-яА-ЯёЁ]/g, '').length >= 2) return t1;
  try {
    const pre2 = await preprocess(dataUrl, false);
    const r2 = await withTimeout(
      w.recognize(pre2, {}, { text: true, tsv: true }),
      90000,
      'Распознавание зависло дольше 90 сек',
    );
    const t2 = rebuildLines(parseTsv(r2.data.tsv as unknown as string)) || cleanOcrText(r2.data.text);
    const c2 = r2.data.confidence ?? 0;
    if (t2.replace(/[^a-zA-Zа-яА-ЯёЁ]/g, '').length >= 2 && c2 > c1) return t2;
  } catch {
    /* второй шанс не вышел — отдаём первый результат */
  }
  return t1;
}

// Вырезать прямоугольник (в пикселях исходника) из dataUrl-картинки.
export async function cropDataUrl(
  src: string,
  sx: number,
  sy: number,
  sw: number,
  sh: number,
): Promise<string> {
  const img = new Image();
  await new Promise<void>((res, rej) => {
    img.onload = () => res();
    img.onerror = () => rej(new Error('img load'));
    img.src = src;
  });
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(sw));
  canvas.height = Math.max(1, Math.round(sh));
  const ctx = canvas.getContext('2d')!;
  ctx.drawImage(img, sx, sy, sw, sh, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL('image/png');
}
