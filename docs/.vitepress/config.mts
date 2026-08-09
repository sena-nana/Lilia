import { defineConfig } from "vitepress";

const githubUrl = "https://github.com/sena-nana/LiliaCode";

export default defineConfig({
  base: "/LiliaCode/",
  title: "LiliaCode",
  description: "面向代码工程的 Agent 协同桌面客户端。",
  cleanUrls: true,
  srcExclude: ["design/**", "github/**"],
  lastUpdated: true,
  sitemap: {
    hostname: "https://sena-nana.github.io/LiliaCode/",
  },
  head: [
    ["link", { rel: "icon", href: "/LiliaCode/logo.png" }],
    ["meta", { name: "theme-color", content: "#2f6b62" }],
  ],
  themeConfig: {
    logo: "/logo.png",
    search: {
      provider: "local",
    },
    socialLinks: [{ icon: "github", link: githubUrl }],
  },
  locales: {
    root: {
      label: "简体中文",
      lang: "zh-CN",
      title: "LiliaCode",
      description: "面向代码工程的 Agent 协同桌面客户端。",
      themeConfig: {
        nav: [
          { text: "首页", link: "/" },
          { text: "文档", link: "/guide/product" },
          { text: "GitHub", link: githubUrl },
        ],
        sidebar: {
          "/guide/": [
            {
              text: "核心文档",
              items: [
                { text: "产品定位", link: "/guide/product" },
                { text: "功能状态", link: "/guide/features" },
                { text: "开发启动", link: "/guide/development" },
                { text: "路线图", link: "/guide/roadmap" },
              ],
            },
          ],
        },
        editLink: {
          pattern: `${githubUrl}/edit/main/docs/:path`,
          text: "在 GitHub 上编辑此页",
        },
        lastUpdated: {
          text: "最后更新",
        },
        outline: {
          label: "页面导航",
        },
        docFooter: {
          prev: "上一页",
          next: "下一页",
        },
        langMenuLabel: "切换语言",
        returnToTopLabel: "回到顶部",
        sidebarMenuLabel: "菜单",
        darkModeSwitchLabel: "主题",
        lightModeSwitchTitle: "切换到浅色模式",
        darkModeSwitchTitle: "切换到深色模式",
      },
    },
    en: {
      label: "English",
      lang: "en-US",
      link: "/en/",
      title: "LiliaCode",
      description: "A desktop client for agent-assisted software engineering.",
      themeConfig: {
        nav: [
          { text: "Home", link: "/en/" },
          { text: "Docs", link: "/en/guide/product" },
          { text: "GitHub", link: githubUrl },
        ],
        sidebar: {
          "/en/guide/": [
            {
              text: "Core Docs",
              items: [
                { text: "Product Positioning", link: "/en/guide/product" },
                { text: "Feature Status", link: "/en/guide/features" },
                { text: "Development", link: "/en/guide/development" },
                { text: "Roadmap", link: "/en/guide/roadmap" },
              ],
            },
          ],
        },
        editLink: {
          pattern: `${githubUrl}/edit/main/docs/:path`,
          text: "Edit this page on GitHub",
        },
      },
    },
  },
});
