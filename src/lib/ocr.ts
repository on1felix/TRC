// Локальный OCR через tesseract.js (eng+rus).
// Первый запуск докачивает движок и traineddata, дальше всё офлайн (кэш в IDB).

import { createWorker, type Worker } from 'tesseract.js';

export type OcrStage = 'idle' | 'loading' | 'ready' | 'error';

let workerPromise: Promise<Worker> | null = null;
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

export function ensureOcr(): Promise<Worker> {
  if (!workerPromise) {
    stage = 'loading';
    stageDetail = 'Загружаю движок OCR…';
    publish(0);
    workerPromise = (async () => {
      try {
        const w = await createWorker(['eng', 'rus'], undefined, {
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
        stage = 'ready';
        stageDetail = 'OCR готов';
        publish(1);
        return w;
      } catch (e) {
        // сбрасываем, чтобы следующая попытка попробовала заново
        workerPromise = null;
        stage = 'error';
        stageDetail = `OCR не загрузился: ${e instanceof Error ? e.message : String(e)}`;
        publish(0);
        throw e;
      }
    })();
  }
  return workerPromise;
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

// Предобработка: адаптивный апскейл (мелочь тянем сильнее) + grayscale + порог.
export async function preprocess(dataUrl: string): Promise<string> {
  const img = new Image();
  await new Promise<void>((res, rej) => {
    img.onload = () => res();
    img.onerror = () => rej(new Error('img load'));
    img.src = dataUrl;
  });
  // цель — длинная сторона ~900px: мелкое выделение x6, крупное x2
  const scale = Math.min(6, Math.max(2, 900 / Math.max(1, Math.max(img.width, img.height))));
  const canvas = document.createElement('canvas');
  canvas.width = Math.min(2600, Math.round(img.width * scale));
  canvas.height = Math.min(2600, Math.round(img.height * scale));
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
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

export async function ocrDataUrl(dataUrl: string): Promise<string> {
  const w = await withTimeout(ensureOcr(), 120000, 'OCR грузится дольше 2 минут — проверь интернет');
  const pre = await preprocess(dataUrl);
  const r = await withTimeout(w.recognize(pre), 90000, 'Распознавание зависло дольше 90 сек');
  return (r.data.text || '').replace(/\s+/g, ' ').trim();
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
