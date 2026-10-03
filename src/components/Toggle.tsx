import { motion } from 'framer-motion';

export function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`relative w-11 h-6 rounded-full transition-colors duration-200 shrink-0 ${
        checked ? 'bg-accent' : 'bg-white/10'
      }`}
    >
      <motion.span
        initial={false}
        animate={{ x: checked ? 22 : 2 }}
        transition={{ type: 'spring', stiffness: 500, damping: 32 }}
        className="absolute top-[2px] left-0 w-5 h-5 rounded-full bg-white shadow"
      />
    </button>
  );
}
