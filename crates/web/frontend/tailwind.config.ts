import type { Config } from 'tailwindcss';
import typography from '@tailwindcss/typography';

export default {
  content: [
    './index.html',
    './src/**/*.{ts,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        // Base surfaces
        void: '#07080c',
        surface: {
          DEFAULT: 'rgba(255,255,255,0.03)',
          muted:   'rgba(255,255,255,0.06)',
          hover:   'rgba(255,255,255,0.08)',
          active:  'rgba(0,229,255,0.08)',
        },
        border: {
          DEFAULT: 'rgba(255,255,255,0.08)',
          bright:  'rgba(255,255,255,0.15)',
          focus:   '#00e5ff',
        },
        // Accent palette — cyberpunk
        cyan:    { DEFAULT: '#00e5ff', dim: '#00b8cc', glow: 'rgba(0,229,255,0.35)' },
        magenta: { DEFAULT: '#ff3df5', dim: '#cc00d0', glow: 'rgba(255,61,245,0.35)' },
        lime:    { DEFAULT: '#7cff5c', dim: '#5cbf42', glow: 'rgba(124,255,92,0.35)' },
        violet:  { DEFAULT: '#a371f7', dim: '#7c4ec4', glow: 'rgba(163,113,247,0.35)' },
        amber:   { DEFAULT: '#ffb347', dim: '#cc8a28', glow: 'rgba(255,179,71,0.35)' },
        // Text
        ink: {
          DEFAULT: '#e2e8f0',
          dim:     '#64748b',
          muted:   '#374151',
        },
      },
      fontFamily: {
        sans:  ['Inter', 'system-ui', 'sans-serif'],
        mono:  ['Geist Mono', 'Cascadia Code', 'Fira Code', 'monospace'],
      },
      fontSize: {
        '2xs': ['10px', { lineHeight: '14px' }],
        xs:    ['11px', { lineHeight: '16px' }],
        sm:    ['12px', { lineHeight: '18px' }],
        md:    ['13px', { lineHeight: '20px' }],
        base:  ['14px', { lineHeight: '22px' }],
        lg:    ['15px', { lineHeight: '24px' }],
        xl:    ['16px', { lineHeight: '26px' }],
      },
      spacing: {
        '0.5': '2px',
        '1':   '4px',
        '2':   '8px',
        '3':   '12px',
        '4':   '16px',
        '5':   '20px',
        '6':   '24px',
        '8':   '32px',
      },
      boxShadow: {
        'neon-cyan':    '0 0 20px rgba(0,229,255,0.4), 0 0 40px rgba(0,229,255,0.2)',
        'neon-magenta': '0 0 20px rgba(255,61,245,0.4), 0 0 40px rgba(255,61,245,0.2)',
        'neon-lime':    '0 0 20px rgba(124,255,92,0.4), 0 0 40px rgba(124,255,92,0.2)',
        'sm':           '0 1px 2px rgba(0,0,0,0.4)',
        'md':           '0 4px 16px rgba(0,0,0,0.5)',
        'lg':           '0 8px 32px rgba(0,0,0,0.6)',
        'panel':        '0 0 0 1px rgba(255,255,255,0.06), 0 8px 32px rgba(0,0,0,0.6)',
      },
      backdropBlur: {
        xs: '4px',
        sm: '8px',
        md: '16px',
        xl: '24px',
      },
      borderRadius: {
        sm:  '4px',
        md:  '8px',
        lg:  '12px',
        xl:  '16px',
        '2xl': '20px',
      },
      keyframes: {
        shimmer: {
          '0%':   { transform: 'translateX(-100%)' },
          '100%': { transform: 'translateX(100%)' },
        },
        fadeUp: {
          '0%':   { opacity: '0', transform: 'translateY(8px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        slideInRight: {
          '0%':   { opacity: '0', transform: 'translateX(16px)' },
          '100%': { opacity: '1', transform: 'translateX(0)' },
        },
        pulse: {
          '0%, 100%': { opacity: '1' },
          '50%':       { opacity: '0.5' },
        },
        loadSlide: {
          '0%':   { transform: 'translateX(-100%)' },
          '100%': { transform: 'translateX(400%)' },
        },
        orbit: {
          '0%':   { transform: 'rotate(0deg) translateX(12px) rotate(0deg)' },
          '100%': { transform: 'rotate(360deg) translateX(12px) rotate(-360deg)' },
        },
        toastIn: {
          '0%':   { opacity: '0', transform: 'translateX(-50%) translateY(10px)' },
          '100%': { opacity: '1', transform: 'translateX(-50%) translateY(0)' },
        },
      },
      animation: {
        shimmer:       'shimmer 1.5s ease-in-out infinite',
        fadeUp:        'fadeUp 0.25s cubic-bezier(0.16,1,0.3,1)',
        slideInRight:  'slideInRight 0.25s cubic-bezier(0.16,1,0.3,1)',
        pulse:         'pulse 2s ease-in-out infinite',
        loadSlide:     'loadSlide 1.2s ease-in-out infinite',
        orbit:         'orbit 1.2s linear infinite',
        toastIn:       'toastIn 0.3s cubic-bezier(0.16,1,0.3,1)',
      },
    },
  },
  plugins: [typography],
} satisfies Config;
