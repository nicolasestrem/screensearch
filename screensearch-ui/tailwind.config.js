/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      fontFamily: {
        sans: ['Geist', 'ui-sans-serif', 'system-ui', '-apple-system', 'sans-serif'],
        serif: ['Newsreader', 'Iowan Old Style', 'Georgia', 'serif'],
        mono: ['Geist Mono', 'ui-monospace', 'monospace'],
      },
      fontSize: {
        'display-lg': ['3rem', { lineHeight: '1.1', letterSpacing: '-0.02em' }],
        'display': ['2.25rem', { lineHeight: '1.2', letterSpacing: '-0.01em' }],
        'title': ['1.5rem', { lineHeight: '1.3', fontWeight: '600' }],
      },
      colors: {
        // Core design system colors from screensearch-website
        ink: "hsl(var(--ink) / <alpha-value>)",
        "ink-2": "hsl(var(--ink-2) / <alpha-value>)",
        muted: "hsl(var(--muted) / <alpha-value>)",
        rule: "hsl(var(--rule) / <alpha-value>)",
        "rule-2": "hsl(var(--rule-2) / <alpha-value>)",
        paper: "hsl(var(--paper) / <alpha-value>)",
        "paper-2": "hsl(var(--paper-2) / <alpha-value>)",
        accent: "hsl(var(--accent) / <alpha-value>)",
        "accent-ink": "hsl(var(--accent-ink) / <alpha-value>)",
        warn: "hsl(var(--warn) / <alpha-value>)",
        good: "hsl(var(--good) / <alpha-value>)",
        
        // Semantic mapping for existing UI components
        border: "hsl(var(--rule))",
        input: "hsl(var(--rule-2))",
        ring: "hsl(var(--accent))",
        background: "hsl(var(--paper))",
        foreground: "hsl(var(--ink))",
        primary: {
          DEFAULT: "hsl(var(--accent-ink))",
          foreground: "hsl(var(--paper))",
        },
        secondary: {
          DEFAULT: "hsl(var(--paper-2))",
          foreground: "hsl(var(--ink))",
        },
        destructive: {
          DEFAULT: "hsl(var(--warn))",
          foreground: "hsl(var(--paper))",
        },
        card: {
          DEFAULT: "hsl(var(--paper))",
          foreground: "hsl(var(--ink))",
        },
        surface: {
          1: "hsl(var(--paper))",
          2: "hsl(var(--paper-2))",
          3: "hsl(var(--rule))",
        },
      },
      borderRadius: {
        // Brutalist style uses sharp corners or very minimal radii
        lg: "0",
        md: "0",
        sm: "0",
        '2xl': '0',
        '3xl': '0',
        full: '9999px',
      },
      boxShadow: {
        // Very subtle or hard shadows
        sm: "0 1px 2px 0 rgba(0, 0, 0, 0.05)",
        DEFAULT: "0 1px 3px 0 rgba(0, 0, 0, 0.1), 0 1px 2px -1px rgba(0, 0, 0, 0.1)",
        md: "0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -2px rgba(0, 0, 0, 0.1)",
        lg: "0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -4px rgba(0, 0, 0, 0.1)",
        hard: "4px 4px 0px 0px rgba(0,0,0,1)",
      },
      keyframes: {
        "fade-in": {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
        "slide-in": {
          "0%": { transform: "translateY(10px)", opacity: "0" },
          "100%": { transform: "translateY(0)", opacity: "1" },
        },
        "fade-in-up": {
          "0%": { opacity: "0", transform: "translateY(10px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
      },
      animation: {
        "fade-in": "fade-in 0.2s ease-out",
        "slide-in": "slide-in 0.2s ease-out",
        "fade-in-up": "fade-in-up 0.3s ease-out",
      },
    },
  },
  plugins: [
    require('tailwindcss-animate'),
    require('@tailwindcss/typography'),
  ],
}
