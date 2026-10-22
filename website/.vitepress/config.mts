import { defineConfig } from "vitepress";
import { withMermaid } from "vitepress-plugin-mermaid";
import llmstxt from "vitepress-plugin-llms";

const repo = "https://github.com/alex-oleshkevich/glimpse";

const developers = [
  {
    text: "For developers",
    items: [
      { text: "How glimpse fits together", link: "/developers/" },
      { text: "How an applet runs", link: "/developers/exec-applets" },
      { text: "How development mode works", link: "/developers/dev-mode" },
      { text: "What an applet can do", link: "/developers/trust" },
    ],
  },
  {
    text: "Reference",
    items: [
      { text: "systemd and D-Bus", link: "/developers/systemd" },
      { text: "Installed files", link: "/developers/files" },
      { text: "Applet protocol", link: "/developers/protocol" },
    ],
  },
  {
    text: "Contribute",
    items: [
      { text: "Build glimpse from source", link: "/contribute/build" },
      { text: "Translate glimpse", link: "/contribute/translate" },
      { text: "Design principles", link: "/contribute/principles" },
      { text: "How the lock screen works", link: "/contribute/lock-screen" },
    ],
  },
];

export default withMermaid(
  defineConfig({
    title: "glimpse",
    description: "A desktop shell for Wayland: panel, notifications, wallpaper, lock screen and more.",
    base: "/glimpse/",
    lang: "en-US",
    cleanUrls: true,
    lastUpdated: true,
    head: [
      ["link", { rel: "icon", type: "image/svg+xml", href: "/glimpse/assets/logo.svg" }],
      ["meta", { name: "theme-color", content: "#1a5fb4" }],
    ],
    vite: {
      plugins: [llmstxt()],
      build: { chunkSizeWarningLimit: 4096 },
      optimizeDeps: { include: ["mermaid > fastdom", "mermaid > fastdom/extensions/fastdom-promised.js"] },
    },
    themeConfig: {
      logo: "/assets/logo.svg",
      nav: [
        { text: "Get started", link: "/start/", activeMatch: "^/start/" },
        { text: "Use glimpse", link: "/guide/", activeMatch: "^/guide/" },
        { text: "Configuration", link: "/config/", activeMatch: "^/config/" },
        { text: "Build applets", link: "/applets/", activeMatch: "^/applets/" },
        {
          text: "For developers",
          link: "/developers/",
          activeMatch: "^/(developers|contribute)/",
        },
        { text: "Ecosystem", link: "/ecosystem/", activeMatch: "^/ecosystem/" },
      ],
      sidebar: {
        "/guide/": [
          {
            text: "Use glimpse",
            items: [
              { text: "Overview", link: "/guide/" },
              { text: "Arrange your panels", link: "/guide/panels" },
              { text: "Show a world clock next to your clock", link: "/guide/second-instance" },
              { text: "Change how glimpse looks", link: "/guide/theme" },
              { text: "Write a theme", link: "/guide/theming" },
              { text: "Set a light and dark wallpaper", link: "/guide/wallpaper" },
              { text: "Show the weather for your city", link: "/guide/weather" },
              { text: "Manage notifications", link: "/guide/notifications" },
              { text: "Lock the screen and handle idle", link: "/guide/lock-idle" },
              { text: "Set up the night light", link: "/guide/night-light" },
              { text: "Bind glimpse to keys", link: "/guide/keys" },
              { text: "Troubleshooting", link: "/guide/troubleshooting" },
            ],
          },
        ],
        "/applets/": [
          {
            text: "Build applets",
            items: [{ text: "Choosing an applet kind", link: "/applets/" }],
          },
          {
            text: "Command applets",
            items: [
              { text: "Add a screenshot button", link: "/applets/command/" },
              { text: "More command buttons", link: "/applets/command/recipes" },
            ],
          },
          {
            text: "Exec applets",
            items: [
              { text: "Build a todo applet", link: "/applets/exec/" },
              { text: "Start in Python, Go or Rust", link: "/applets/exec/start" },
              { text: "Develop with live reload", link: "/applets/exec/live-reload" },
              { text: "Configure an applet in the panel", link: "/applets/exec/configure" },
              { text: "Keep data between restarts", link: "/applets/exec/data" },
              { text: "Notify, copy, open links and launch apps", link: "/applets/exec/host" },
              { text: "Test an applet", link: "/applets/exec/testing" },
              { text: "Debug a running applet", link: "/applets/exec/debug" },
              { text: "Package and share an applet", link: "/applets/exec/share" },
            ],
          },
          {
            text: "Reference",
            items: [
              { text: "Elements", link: "/applets/exec/reference/elements" },
              { text: "TypeScript SDK", link: "/applets/exec/reference/sdk" },
              { text: "Python SDK", link: "/applets/exec/reference/python" },
              { text: "Go SDK", link: "/applets/exec/reference/go" },
              { text: "Rust SDK", link: "/applets/exec/reference/rust" },
              { text: "Desktop entry keys", link: "/applets/exec/reference/desktop-entry" },
              { text: "Package format", link: "/applets/exec/reference/package" },
            ],
          },
        ],
        "/ecosystem/": [
          {
            text: "Ecosystem",
            items: [
              { text: "Overview", link: "/ecosystem/" },
              { text: "Screenshots", link: "/ecosystem/shot" },
              { text: "Color picker", link: "/ecosystem/picker" },
              { text: "Ruler", link: "/ecosystem/ruler" },
              { text: "Magnifier", link: "/ecosystem/lens" },
            ],
          },
        ],
        "/developers/": developers,
        "/contribute/": developers,
        "/start/": [
          {
            text: "Get started",
            items: [
              { text: "What is glimpse", link: "/start/" },
              { text: "Install", link: "/start/install" },
              { text: "Your first session", link: "/start/first-session" },
            ],
          },
        ],
        "/config/": [
          {
            text: "Configuration",
            items: [
              { text: "How configuration works", link: "/config/" },
              { text: "Panels", link: "/config/panels" },
              { text: "Theme tokens", link: "/config/theme-tokens" },
            ],
          },
          {
            text: "Applets",
            items: [
              { text: "All applets", link: "/config/applets/" },
              { text: "System", link: "/config/applets/system" },
              { text: "Workspaces and windows", link: "/config/applets/workspaces" },
              { text: "Information", link: "/config/applets/information" },
              { text: "Tools", link: "/config/applets/tools" },
              { text: "Command and exec", link: "/config/applets/custom" },
            ],
          },
          {
            text: "Services",
            items: [
              { text: "Appearance, region and location", link: "/config/services/appearance" },
              { text: "Notifications", link: "/config/services/notifications" },
              { text: "Lock screen", link: "/config/services/lock" },
              { text: "Idle", link: "/config/services/idle" },
              { text: "Night light", link: "/config/services/night-light" },
              { text: "Wallpaper", link: "/config/services/wallpaper" },
              { text: "Screenshots", link: "/config/services/screenshots" },
            ],
          },
          {
            text: "Command line",
            items: [
              { text: "glimpsectl", link: "/config/cli/glimpsectl" },
              { text: "glimpse-shot", link: "/config/cli/glimpse-shot" },
              { text: "glimpse-lens", link: "/config/cli/glimpse-lens" },
              { text: "glimpse-lock", link: "/config/cli/glimpse-lock" },
            ],
          },
        ],
      },
      socialLinks: [{ icon: "github", link: repo }],
      editLink: {
        pattern: `${repo}/edit/master/website/:path`,
        text: "Edit this page on GitHub",
      },
      search: { provider: "local" },
      outline: { level: [2, 3] },
      footer: {
        message: "Released under the BSD 3-Clause license.",
      },
    },
  }),
);
