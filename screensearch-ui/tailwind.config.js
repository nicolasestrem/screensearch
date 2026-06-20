import typography from '@tailwindcss/typography'

/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // Command Deck — Warm Graphite + Signal Orange
        void: '#17171B',
        panel: '#1F1F25',
        panel2: '#1A1A1F',
        rule: '#32323C',
        rule2: '#3E3E48',
        ink: '#ECE7DF',
        ink2: '#C9C3B8',
        muted: '#A39E94',
        faint: '#928C81',
        accent: '#FF6A3D',
        accent2: '#C24E2A',
        ok: '#7FB87A',
        warn: '#E0A33E',
        alert: '#E5564C',
        act: {
          code: '#E8853D',
          research: '#5E8BA8',
          comms: '#C9A24B',
          media: '#9B7CB8',
          reading: '#6FA8A0',
          idle: '#4A4A52',
        },
      },
      fontFamily: {
        display: ['Bahnschrift', '"DIN Alternate"', '"Segoe UI"', 'sans-serif'],
        sans: ['"Segoe UI"', 'system-ui', '-apple-system', 'sans-serif'],
        mono: ['Consolas', '"Cascadia Mono"', 'ui-monospace', 'monospace'],
      },
      borderRadius: {
        none: '0',
        DEFAULT: '0',
        sm: '0',
        md: '0',
      },
      letterSpacing: {
        label: '0.16em',
      },
      keyframes: {
        'pulse-rec': { '0%,100%': { opacity: '1' }, '50%': { opacity: '0.35' } },
        'row-in': { '0%': { opacity: '0', transform: 'translateY(4px)' }, '100%': { opacity: '1', transform: 'none' } },
        blink: { '50%': { opacity: '0' } },
      },
      animation: {
        'pulse-rec': 'pulse-rec 2.2s ease-in-out infinite',
        'row-in': 'row-in 0.28s ease-out both',
        blink: 'blink 1.1s steps(1) infinite',
      },
    },
  },
  plugins: [typography],
}
