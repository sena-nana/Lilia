import {
  createLiliaSettingsModel,
  type LiliaSettingsTabInput,
} from "@lilia/ui/settings";
import {
  resolveLiliaIcon,
  type IconInput,
  type LiliaUiConfig,
} from "@lilia/ui/shell";
import desktopPackage from "../package.json";

type AppSettingsTab = Omit<LiliaSettingsTabInput, "icon"> & { icon: IconInput };

const settingsTabs = [
  { key: "appearance", label: "外观", icon: "palette" },
  { key: "window", label: "窗口", icon: "panel-top" },
  { key: "providers", label: "连接", icon: "network" },
  { key: "credentials", label: "凭据", icon: "key-round" },
  { key: "remote-control", label: "Android 远控", icon: "monitor-smartphone" },
  { key: "assistant", label: "Provider 配置", icon: "sparkles" },
  { key: "model-config", label: "模型配置", icon: "brain" },
  { key: "agent", label: "Agent", icon: "bot" },
  { key: "shared-services", label: "共享 Services", icon: "server" },
  { key: "quota", label: "额度", icon: "gauge" },
  { key: "plugin-skills", label: "技能", icon: "sparkles", props: { section: "skills" } },
  { key: "plugin-packages", label: "插件", icon: "puzzle", props: { section: "packages" } },
  { key: "plugin-hooks", label: "Hooks", icon: "workflow", props: { section: "hooks" } },
  { key: "plugin-mcp", label: "MCP", icon: "server", props: { section: "mcp" } },
  { key: "import", label: "导入对话", icon: "download" },
  { key: "project", label: "项目", icon: "folder-cog" },
  { key: "about", label: "关于", icon: "info" },
] satisfies readonly AppSettingsTab[];

export const settingsModel = createLiliaSettingsModel({
  path: "/settings",
  aliases: {
    plugins: "plugin-skills",
  },
  defaultTab: "appearance",
  fullPageTabs: [
    "plugin-skills",
    "plugin-packages",
    "plugin-hooks",
    "plugin-mcp",
    "import",
  ],
  hideHeader: true,
  tabs: settingsTabs.map((tab) => ({
    ...tab,
    icon: resolveLiliaIcon(tab.icon),
  })),
  sections: {
    appearance: () => import("./pages/settings/AppearanceSection.vue"),
    window: () => import("./pages/settings/PopupWindowSection.vue"),
    providers: () => import("./pages/settings/ProviderConnectionSection.vue"),
    credentials: () => import("./pages/settings/NativeCredentialSection.vue"),
    "remote-control": () => import("./pages/settings/RemoteControlSection.vue"),
    assistant: () => import("./pages/settings/AssistantAISection.vue"),
    "model-config": () => import("./pages/settings/ModelConfigurationSection.vue"),
    agent: () => import("./pages/settings/AgentInteractionSection.vue"),
    "shared-services": () => import("./pages/settings/NativeSharedServicesSection.vue"),
    quota: () => import("./pages/settings/QuotaUsageSection.vue"),
    "plugin-skills": () => import("./pages/Plugins.vue"),
    "plugin-packages": () => import("./pages/Plugins.vue"),
    "plugin-hooks": () => import("./pages/Plugins.vue"),
    "plugin-mcp": () => import("./pages/Plugins.vue"),
    import: () => import("./pages/ConversationImport.vue"),
    project: () => import("./pages/settings/ProjectPreferencesSection.vue"),
    about: () => import("./pages/settings/AboutSection.vue"),
  },
});

export const appConfig = {
  appName: "lilia",
  productTitle: "Lilia",
  version: desktopPackage.version,
  storageKeyPrefix: "lilia",
} satisfies LiliaUiConfig;
