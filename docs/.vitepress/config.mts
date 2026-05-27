import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Glimpse',
  description: 'A polished desktop shell toolkit for Niri.',
  base: '/glimpse/',
  cleanUrls: true,
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/glimpse/favicon.svg' }],
    ['meta', { name: 'theme-color', content: '#3584e4' }]
  ],
  srcExclude: ['superpowers/**'],
  themeConfig: {
    nav: [
      { text: 'Motivation', link: '/motivation' },
      { text: 'Installation', link: '/installation' },
      { text: 'Panels and Applets', link: '/configuration' },
      { text: 'Applets', link: '/applets/' },
      { text: 'Theming', link: '/theming' },
      { text: 'Wallpaper', link: '/wallpaper' },
      { text: 'Lock', link: '/lock' },
      { text: 'GitHub', link: 'https://github.com/alex-oleshkevich/glimpse' }
    ],
    sidebar: [
      {
        text: 'Start Here',
        items: [
          { text: 'Introduction', link: '/' },
          { text: 'Motivation', link: '/motivation' },
          { text: 'Installation', link: '/installation' },
          { text: 'Calendar Sources', link: '/calendar' },
          { text: 'Theming', link: '/theming' }
        ]
      },
      {
        text: 'Configuration',
        items: [
          { text: 'Panels and Applets', link: '/configuration' },
          { text: 'Wallpaper', link: '/wallpaper' },
          { text: 'Lock Screen', link: '/lock' }
        ]
      },
      {
        text: 'Applets',
        items: [
          { text: 'Applet Reference', link: '/applets/' },
          { text: 'Custom Applets', link: '/custom-applets/' },
          { text: 'Command Applet', link: '/custom-applets/command' }
        ]
      },
      {
        text: 'Exec',
        items: [
          { text: 'Getting Started', link: '/custom-applets/getting-started' },
          { text: 'Tooling', link: '/custom-applets/tooling' },
          { text: 'Exec Applet', link: '/custom-applets/exec' },
          { text: 'SDK', link: '/applets/exec-sdk' },
          { text: 'Line Protocol', link: '/custom-applets/exec-protocol' },
          { text: 'Components', link: '/custom-applets/exec-components' }
        ]
      },
      {
        text: 'Services',
        items: [
          { text: 'Idle', link: '/idle' },
          { text: 'Sunset', link: '/sunset' }
        ]
      },
      {
        text: 'References',
        items: [
          { text: 'IPC Developer Spec', link: '/ipc' },
          { text: 'Packaging Guide', link: '/packaging' },
          { text: 'LLM References', link: '/llms/' }
        ]
      }
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/alex-oleshkevich/glimpse' }
    ],
    search: {
      provider: 'local'
    }
  }
})
