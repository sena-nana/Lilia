/// <reference types="vitest" />
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";

// @ts-expect-error process 是 Node.js 全局对象
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process 是 Node.js 全局对象
const port = Number.parseInt(process.env.LILIA_DEV_PORT ?? "1420", 10);
// @ts-expect-error process 是 Node.js 全局对象
const strictPort = process.env.LILIA_DEV_STRICT_PORT === "1";
const katexMinifiedEntry = fileURLToPath(
  new URL("../../node_modules/katex/dist/katex.min.js", import.meta.url),
);

const devMockAliases = {
  "@tauri-apps/api/core": fileURLToPath(
    new URL("./src/tauri/devMock.ts", import.meta.url),
  ),
  "@tauri-apps/api/event": fileURLToPath(
    new URL("./src/tauri/devMock.ts", import.meta.url),
  ),
  "@tauri-apps/api/window": fileURLToPath(
    new URL("./src/tauri/devMock.ts", import.meta.url),
  ),
  "@tauri-apps/api/path": fileURLToPath(
    new URL("./src/tauri/devMock.ts", import.meta.url),
  ),
  "@tauri-apps/api/webview": fileURLToPath(
    new URL("./src/tauri/devMock.ts", import.meta.url),
  ),
  "@tauri-apps/api/dpi": fileURLToPath(
    new URL("./src/tauri/devMock.ts", import.meta.url),
  ),
};

function chunkFileNames(chunk: { moduleIds?: string[] }): string {
  const moduleIds = (chunk.moduleIds ?? []).map((id) => id.replace(/\\/g, "/"));
  if (moduleIds.some((id) => id.includes("/node_modules/@mermaid-js/parser/"))) {
    return "assets/diagram-mermaid-parser-[hash].js";
  }
  if (moduleIds.some((id) => id.includes("/node_modules/@vue-flow/core/"))) {
    return "assets/vendor-vue-flow-[hash].js";
  }
  if (moduleIds.some((id) => id.endsWith("/node_modules/katex/dist/katex.min.js"))) {
    return "assets/vendor-katex-[hash].js";
  }
  if (moduleIds.some((id) => id.includes("/node_modules/mermaid/") ||
    id.includes("/node_modules/@mermaid-js/"))) {
    return "assets/diagram-mermaid-[hash].js";
  }
  return "assets/[name]-[hash].js";
}

// https://vite.dev/config/
export default defineConfig(async ({ command, mode }) => ({
  plugins: [
    {
      name: "lilia-katex-minified-entry",
      enforce: "pre",
      resolveId(source) {
        return source === "katex" ? katexMinifiedEntry : null;
      },
    },
    vue(),
  ],

  resolve: {
    alias: {
      ...(command === "serve" && mode !== "test" && !strictPort ? devMockAliases : {}),
      "@lilia/contracts/agentInteraction.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/agent-interaction.ts", import.meta.url),
      ),
      "@lilia/contracts/agentInteractionContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/agentInteractionContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/appCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/appCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/appEventsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/appEventsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/askUserContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/askUserContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/architectureContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/architectureContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/chatBackendsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/chatBackendsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/chatCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/chatCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/chatEventsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/chatEventsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/conversationContextContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/conversationContextContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/githubCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/githubCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/liliaAgentProtocol.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/liliaAgentProtocol.mjs", import.meta.url),
      ),
      "@lilia/contracts/liliaWorkflowContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/liliaWorkflowContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/liliaCodeCore.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/liliaCodeCore.mjs", import.meta.url),
      ),
      "@lilia/contracts/milestoneCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/milestoneCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/permissionModes.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/permissionModes.vite.mjs", import.meta.url),
      ),
      "@lilia/contracts/promptContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/promptContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/provider.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/providerRuntime.mjs", import.meta.url),
      ),
      "@lilia/contracts/providerCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/providerCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/projectCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/projectCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/productCoreContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/productCoreContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/quotaContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/quotaContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/remoteControlCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/remoteControlCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/runnerProtocolContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/runnerProtocolContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/runtimeCommandContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/runtimeCommandContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/sessionManagementContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/sessionManagementContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/suggestionsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/suggestionsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/systemCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/systemCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/taskCommandsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/taskCommandsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/taskEventsContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/taskEventsContract.mjs", import.meta.url),
      ),
      "@lilia/contracts/timelineContract.mjs": fileURLToPath(
        new URL("../../packages/contracts/src/timelineContract.mjs", import.meta.url),
      ),
      "@lilia/contracts": fileURLToPath(
        new URL("../../packages/contracts/src/index.ts", import.meta.url),
      ),
    },
    dedupe: ["vue", "vue-router", "@vue/runtime-core", "@vue/runtime-dom", "@lucide/vue"],
  },

  // 这些 Vite 选项面向 Tauri 开发，只在 `tauri dev` 或 `tauri build` 中生效
  clearScreen: false,
  server: {
    port: Number.isFinite(port) ? port : 1420,
    strictPort,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    environment: "jsdom",
    execArgv: ["--no-experimental-webstorage"],
    setupFiles: ["./tests/setupTests.ts"],
  },
  build: {
    rollupOptions: {
      output: {
        chunkFileNames,
      },
    },
  },
}));
