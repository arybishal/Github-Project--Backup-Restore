/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        background: 'var(--background)',
        foreground: 'var(--foreground)',
        secondary: 'var(--text-secondary)',
        muted: 'var(--text-muted)',
        primary: 'var(--primary)',
        'primary-soft': 'var(--primary-soft)',
        pale: 'var(--pale-yellow)',
        soft: 'var(--soft-black)',
        border: 'var(--border)',
        surface: 'var(--surface)',
        success: 'var(--success)',
        warning: 'var(--warning)',
        error: 'var(--error)',
      },
      borderColor: {
        DEFAULT: '#EAEAEA',
      },
      boxShadow: {
        soft: '0 1px 2px rgba(17, 17, 17, 0.04), 0 4px 14px rgba(17, 17, 17, 0.05)',
        lift: '0 2px 4px rgba(17, 17, 17, 0.05), 0 8px 24px rgba(17, 17, 17, 0.07)',
      },
      transitionDuration: {
        DEFAULT: '150ms',
      },
    },
  },
  plugins: [],
};
