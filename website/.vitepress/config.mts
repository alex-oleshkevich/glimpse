import { defineConfig } from "vitepress";
import { withMermaid } from "vitepress-plugin-mermaid";
import llmstxt from "vitepress-plugin-llms";

const repo = "https://github.com/alex-oleshkevich/glimpse";

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
        { text: "Configuration", link: "/config/", activeMatch: "^/config/" },
        { text: "Ecosystem", link: "/ecosystem/", activeMatch: "^/ecosystem/" },
      ],
      sidebar: {
        "/start/": [
          {
            text: "Get started",
            items: [
              { text: "What is glimpse", link: "/start/" },
              { text: "Install", link: "/start/install" },
              { text: "Your first session", link: "/start/first-session" },
              { text: "Upgrade and uninstall", link: "/start/upgrade" },
            ],
          },
        ],
        "/config/": [
          {
            text: "Configuration",
            items: [
              { text: "How configuration works", link: "/config/" },
              { text: "Panels", link: "/config/panels" },
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
