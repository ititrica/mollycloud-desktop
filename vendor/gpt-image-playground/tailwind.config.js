import colors from 'tailwindcss/colors';

/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'media',
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}', './node_modules/streamdown/dist/*.js'],
  theme: {
    extend: {
      colors: {
        ...(process.env.VITE_MOLLY_EMBEDDED === 'true' ? {
          blue: {
            50: 'hsl(var(--molly-accent-hsl) / <alpha-value>)', 100: 'hsl(var(--molly-accent-hsl) / <alpha-value>)',
            200: 'hsl(var(--molly-accent-border-hsl) / <alpha-value>)', 300: 'hsl(var(--molly-accent-border-hsl) / <alpha-value>)',
            400: 'hsl(var(--molly-primary-hsl) / <alpha-value>)', 500: 'hsl(var(--molly-primary-hsl) / <alpha-value>)',
            600: 'hsl(var(--molly-primary-hover-hsl) / <alpha-value>)', 700: 'hsl(var(--molly-primary-pressed-hsl) / <alpha-value>)',
            800: 'hsl(var(--molly-ring-hsl) / <alpha-value>)', 900: 'hsl(var(--molly-primary-foreground-hsl) / <alpha-value>)',
            950: 'hsl(var(--molly-primary-foreground-hsl) / <alpha-value>)',
          },
        } : {}),
        background: 'hsl(var(--background) / <alpha-value>)',
        border: 'hsl(var(--border) / <alpha-value>)',
        gray: colors.zinc,
        foreground: 'hsl(var(--foreground) / <alpha-value>)',
        input: 'hsl(var(--input) / <alpha-value>)',
        muted: {
          DEFAULT: 'hsl(var(--muted) / <alpha-value>)',
          foreground: 'hsl(var(--muted-foreground) / <alpha-value>)',
        },
        primary: {
          DEFAULT: 'hsl(var(--primary) / <alpha-value>)',
          foreground: 'hsl(var(--primary-foreground) / <alpha-value>)',
        },
        sidebar: {
          DEFAULT: 'hsl(var(--sidebar) / <alpha-value>)',
          foreground: 'hsl(var(--sidebar-foreground) / <alpha-value>)',
        },
      },
      fontFamily: {
        sans: ['var(--font-ui-sans)'],
        mono: ['var(--font-mono)'],
      },
    },
  },
  plugins: [],
}
