import { motion } from 'framer-motion';
import { Download, CheckCircle2, ArrowRight } from 'lucide-react';
import type { UpdateInfo, UpdateProgress } from '../api';

interface UpdateModalProps {
  info: UpdateInfo;
  progress: UpdateProgress | null;
  restarting: boolean;
  error?: string | null;
}

const mb = (bytes: number) => (bytes / 1048576).toFixed(1);

export function UpdateModal({ info, progress, restarting, error }: UpdateModalProps) {
  const percent = progress ? Math.min(progress.percent, 100) : 0;

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.25 }}
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/70"
    >
      <motion.div
        initial={{ scale: 0.92, opacity: 0, y: 14 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        exit={{ scale: 0.95, opacity: 0 }}
        transition={{ duration: 0.3, ease: 'easeOut' }}
        className="w-[420px] rounded-2xl bg-[#1a1e26] border border-border p-7 shadow-2xl shadow-black/70 gradient-border"
      >
        <div className="flex items-center gap-3.5 mb-5">
          <motion.div
            animate={{ scale: [1, 1.07, 1] }}
            transition={{ duration: 1.8, repeat: Infinity, ease: 'easeInOut' }}
            className="w-12 h-12 rounded-xl bg-accent/15 border border-accent/40 flex items-center justify-center flex-shrink-0
              shadow-[0_0_18px_rgba(94,143,194,0.3)]"
          >
            {restarting ? (
              <CheckCircle2 className="w-6 h-6 text-accent" />
            ) : (
              <Download className="w-6 h-6 text-accent" />
            )}
          </motion.div>
          <div>
            <div className="text-lg font-semibold text-white/90">
              {restarting ? 'Обновление установлено' : 'Доступно обновление'}
            </div>
            {restarting ? (
              <div className="text-xs text-text-secondary mt-0.5">Перезапускаю TRC…</div>
            ) : (
              <div className="flex items-center gap-2 mt-0.5">
                <span className="text-xs text-text-secondary tabular-nums">v{info.current}</span>
                <ArrowRight className="w-3.5 h-3.5 text-accent" />
                <span className="text-xs text-white/90 font-semibold tabular-nums">v{info.latest}</span>
              </div>
            )}
          </div>
        </div>

        <div className="h-2.5 rounded-full bg-white/[0.06] overflow-hidden">
          <div
            className="h-full rounded-full bg-gradient-to-r from-accent/60 to-accent
              shadow-[0_0_12px_rgba(94,143,194,0.55)] transition-[width] duration-300 ease-out"
            style={{ width: `${restarting ? 100 : percent}%` }}
          />
        </div>

        <div className="flex items-center justify-between mt-3">
          <span className="text-sm font-semibold text-white/90 tabular-nums">
            {percent.toFixed(1)}%
          </span>
          {!restarting && progress && (
            <span className="text-xs text-accent font-medium tabular-nums">
              {progress.speed_mbps.toFixed(1)} МБ/с
            </span>
          )}
        </div>

        <div className="flex items-center justify-between mt-1.5">
          <span className="text-[11px] text-text-muted">
            {progress
              ? `Скачано ${mb(progress.downloaded)} из ${mb(info.size || progress.total)} МБ`
              : 'Подготовка…'}
          </span>
          <span className="text-[11px] text-text-muted">TRC</span>
        </div>

        {error && (
          <div className="mt-4 text-xs text-danger leading-relaxed bg-danger/10 border border-danger/30 rounded-lg px-3 py-2">
            {error}
          </div>
        )}
      </motion.div>
    </motion.div>
  );
}
