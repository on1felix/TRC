/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        bg: {
          DEFAULT: '#16181d',
          2: '#181c23',
          3: '#1c212b',
        },
        card: {
          DEFAULT: '#1a1e26',
          hover: '#1f242e',
        },
        accent: {
          DEFAULT: '#5e8fc2',
          dark: '#4a739f',
          light: '#8fb4dd',
          glow: 'rgba(94, 143, 194, 0.25)',
        },
        text: {
          DEFAULT: '#ffffff',
          2: '#cfd4dc',
          secondary: '#8a8f9c',
          muted: '#5a606e',
        },
        border: {
          DEFAULT: '#262c38',
        },
        success: '#34c759',
        danger: '#ff3b30',
        warning: '#ff9500',
      },
      fontFamily: {
        sans: ['Onest', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Consolas', 'monospace'],
      },
      borderRadius: {
        sm: '8px',
        base: '12px',
        lg: '16px',
        xl: '24px',
        pill: '48px',
      },
      keyframes: {
        'screen-in': {
          '0%': { opacity: '0', transform: 'translateY(8px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        shimmer: {
          '0%': { backgroundPosition: '-200% 0' },
          '100%': { backgroundPosition: '200% 0' },
        },
      },
      animation: {
        'screen-in': 'screen-in 0.3s ease-out',
        'fade-in': 'fade-in 0.3s ease-out',
        shimmer: 'shimmer 2s linear infinite',
      },
    },
  },
  plugins: [],
};
