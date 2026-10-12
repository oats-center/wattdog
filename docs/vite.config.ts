import adapter from '@sveltejs/adapter-static'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'
import { defaultTheme } from '@sveltepress/theme-default'
import { sveltepress } from '@sveltepress/vite'
import { defineConfig } from 'vite'
import sidebar from './sidebar.json' with { type: 'json' }

// GitHub Pages serves the site from /<repository>. CI sets BASE_PATH from the
// repository name; local builds default to the canonical name.
const base = process.env.BASE_PATH ?? '/wattdog'

export default defineConfig({
  plugins: [
    sveltepress({
      theme: defaultTheme({
        logo: '/logo.svg',
        github: 'https://github.com/oats-center/wattdog',
        editLink: 'https://github.com/oats-center/wattdog/edit/main/docs/src/routes/:route',
        navbar: [
          { title: 'Docs', to: '/docs/' },
          { title: 'Guides', to: '/docs/guides/pikvm-shutdown/' },
          { title: 'Concepts', to: '/docs/concepts/thresholds/' },
          { title: 'Reference', to: '/docs/reference/config/' },
        ],
        sidebar: { '/docs/': sidebar },
        themeColor: {
          light: '#f7f5ee',
          dark: '#111314',
          primary: '#8a6a2c',
          primaryDeep: '#6b5221',
          hover: '#b28d45',
          gradient: { start: '#323536', end: '#b28d45' },
        },
        highlighter: {
          languages: ['bash', 'sh', 'console', 'toml', 'sql', 'hcl', 'ini', 'json', 'svelte', 'ts', 'js', 'md', 'css', 'text'],
          themeLight: 'github-light',
          themeDark: 'github-dark-dimmed',
        },
        preBuildIconifyIcons: {
          tabler: ['bluetooth', 'hourglass', 'list-numbers', 'database', 'chart-line', 'shield-check', 'server-bolt', 'solar-panel', 'antenna', 'flask', 'arrow-right'],
        },
      }),
      siteConfig: {
        title: 'wattdog',
        description: 'A PowerMon watchdog that records every reading and acts when power crosses a threshold.',
      },
      svelteKitOptions: {
        preprocess: [vitePreprocess()],
        adapter: adapter({ pages: 'dist' }),
        paths: { base, relative: false },
      },
    }),
  ],
})
