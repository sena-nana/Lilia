import Server from "@lucide/vue/dist/esm/icons/server.mjs";
import { registerChatSidebarPanel } from "./useChatSidebar";

export function registerSharedServicesChatSidebarPanel(): () => void {
  return registerChatSidebarPanel({
    id: "shared-services",
    title: "工作区工具",
    icon: Server,
    order: 30,
    loader: async () =>
      (await import("../components/chat/SharedServicesSidebarPanel.vue")).default,
  });
}
