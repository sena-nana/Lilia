import { fireEvent, render, waitFor, within } from "@testing-library/vue";
import { createMemoryHistory } from "vue-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  CHAT_CHECK_ENV_COMMAND,
  PROVIDER_CODEX_ACCOUNT_START_LOGIN_COMMAND,
  PROVIDER_CODEX_APP_SERVER_CHECK_UPDATE_COMMAND,
  PROVIDER_CODEX_APP_SERVER_INSTALL_UPDATE_COMMAND,
  PROVIDER_GET_ACTIVE_BACKEND_COMMAND,
  QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND,
} from "@lilia/contracts";
import SidebarConnectionFooter from "../src/components/sidebar/SidebarConnectionFooter.vue";
import { useConnectionStatus } from "../src/composables/useConnectionStatus";
import { createLiliaRouter } from "../src/router";
import {
  mockInvoke,
  setMockActiveBackend,
  setMockCodexAccountQuotaStatus,
  setMockCodexAppServerStatus,
  setMockRouterMode,
} from "./tauriMock";

async function renderFooter(initialRoute = "/") {
  const router = createLiliaRouter(createMemoryHistory());
  await router.push(initialRoute);
  await router.isReady();
  await useConnectionStatus({ probe: false }).refresh(false);
  mockInvoke.mockClear();

  return render(SidebarConnectionFooter, {
    global: {
      plugins: [router],
    },
  });
}

function providerBadge(container: HTMLElement, name: string | RegExp): HTMLElement {
  return within(container).getByRole("link", { name });
}

function officialQuota(overrides: Record<string, unknown> = {}) {
  return {
    available: true,
    connectionMode: "codex-account",
    limitId: "codex",
    limitName: "Codex",
    planType: "Pro",
    rateLimitReachedType: null,
    fiveHour: {
      usedPercent: 42,
      windowDurationMins: 300,
      resetsAt: 1_800_000_000,
    },
    weekly: {
      usedPercent: 91,
      windowDurationMins: 10080,
      resetsAt: 1_800_300_000,
    },
    sparkFiveHour: {
      usedPercent: 12,
      windowDurationMins: 300,
      resetsAt: 1_800_060_000,
    },
    sparkWeekly: {
      usedPercent: 80,
      windowDurationMins: 10080,
      resetsAt: 1_800_360_000,
    },
    credits: {
      hasCredits: true,
      unlimited: false,
      balance: "3",
    },
    sparkCredits: {
      hasCredits: true,
      unlimited: true,
      balance: null,
    },
    rateLimitResetCredits: {
      availableCount: 2,
    },
    accountUsage: {
      summary: {
        lifetimeTokens: 123456,
        peakDailyTokens: 4567,
        longestRunningTurnSec: 540,
        currentStreakDays: 8,
        longestStreakDays: 14,
      },
      dailyUsageBuckets: [
        { startDate: "2026-06-17", tokens: 1200 },
        { startDate: "2026-06-18", tokens: 3400 },
      ],
    },
    usageError: null,
    fetchedAt: 1_800_000_000_000,
    error: null,
    ...overrides,
  };
}

function invokeCount(command: string): number {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === command).length;
}

describe("SidebarConnectionFooter provider quota badge", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it.skip("delays startup connection and remote quota refreshes", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", undefined);
    vi.stubGlobal("cancelAnimationFrame", undefined);
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAccountQuotaStatus(officialQuota());

    await renderFooter();

    expect(invokeCount(CHAT_CHECK_ENV_COMMAND)).toBe(0);
    expect(invokeCount(PROVIDER_GET_ACTIVE_BACKEND_COMMAND)).toBe(0);
    expect(invokeCount(PROVIDER_CODEX_APP_SERVER_CHECK_UPDATE_COMMAND)).toBe(0);
    expect(invokeCount(QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND)).toBe(0);

    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1_200);
    await vi.advanceTimersByTimeAsync(1);

    expect(invokeCount(CHAT_CHECK_ENV_COMMAND)).toBe(1);
    expect(invokeCount(PROVIDER_GET_ACTIVE_BACKEND_COMMAND)).toBe(1);
    expect(invokeCount(PROVIDER_CODEX_APP_SERVER_CHECK_UPDATE_COMMAND)).toBe(0);
    expect(invokeCount(QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND)).toBe(0);

    await vi.advanceTimersByTimeAsync(2_500);
    await vi.advanceTimersByTimeAsync(1);

    expect(invokeCount(PROVIDER_CODEX_APP_SERVER_CHECK_UPDATE_COMMAND)).toBe(1);
    expect(invokeCount(QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND)).toBe(1);
  });

  it.skip("does not show quota rings for Claude", async () => {
    setMockActiveBackend("native-agentkit");
    const view = await renderFooter();

    await waitFor(() => {
      expect(providerBadge(view.container, "Claude API 未配置。点击进入设置。")).toHaveAttribute(
        "aria-label",
        "Claude API 未配置。点击进入设置。",
      );
    });
  });

  it.skip("does not show quota rings when Codex uses API mode", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "api");
    setMockCodexAppServerStatus({
      latestVersion: "0.141.0",
      updateAvailable: true,
      releaseNotes: ["App-server update"],
    });
    const view = await renderFooter();

    await waitFor(() => {
      expect(providerBadge(view.container, "Codex API 未配置。点击进入设置。")).toHaveAttribute(
        "aria-label",
        "Codex API 未配置。点击进入设置。",
      );
    });
    expect(view.queryByRole("button", { name: /更新 Codex app-server/ })).not.toBeInTheDocument();
  });

  it.skip("shows Codex app-server update action with hover details and hides after update", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAppServerStatus({
      version: "codex-cli 0.136.0",
      latestVersion: "0.141.0",
      updateAvailable: true,
      updateState: "ready",
      preparedVersion: "0.141.0",
      releaseNotes: ["App-server clients can list immediate child threads."],
    });

    const view = await renderFooter();

    const updateButton = await view.findByRole("button", { name: /切换 Codex app-server/ });
    expect(updateButton).toHaveAttribute(
      "title",
      "切换 Codex app-server：codex-cli 0.136.0 -> 0.141.0",
    );

    await fireEvent.mouseEnter(updateButton);

    expect(await view.findByRole("tooltip")).toHaveTextContent(
      "App-server clients can list immediate child threads.",
    );

    await fireEvent.click(updateButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PROVIDER_CODEX_APP_SERVER_INSTALL_UPDATE_COMMAND,
        {},
        undefined,
      );
      expect(view.queryByRole("button", { name: /切换 Codex app-server/ })).not.toBeInTheDocument();
    });
  });

  it.skip("shows Codex app-server background download state without applying", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAppServerStatus({
      version: "codex-cli 0.136.0",
      latestVersion: "0.141.0",
      updateAvailable: true,
      updateState: "downloading",
      preparedVersion: "0.141.0",
      updateProgressPercent: 42,
    });

    const view = await renderFooter();

    const updateButton = await view.findByRole("button", { name: /下载 Codex app-server/ });
    expect(updateButton).toBeDisabled();
    expect(updateButton).toHaveAttribute(
      "title",
      "下载 Codex app-server（42%）：codex-cli 0.136.0 -> 0.141.0",
    );
  });

  it.skip("shows Codex app-server failed switch retry from the badge", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAppServerStatus({
      version: "codex-cli 0.136.0",
      latestVersion: "0.141.0",
      updateAvailable: true,
      updateState: "failed",
      preparedVersion: "0.141.0",
      updateError: "创建 Codex 切换链接失败",
    });

    const view = await renderFooter();

    const updateButton = await view.findByRole("button", { name: /重试切换 Codex app-server/ });
    expect(updateButton).toBeEnabled();
    expect(updateButton).toHaveAttribute(
      "title",
      "重试切换 Codex app-server：codex-cli 0.136.0 -> 0.141.0",
    );

    await fireEvent.mouseEnter(updateButton);
    expect(await view.findByRole("tooltip")).toHaveTextContent("创建 Codex 切换链接失败");

    await fireEvent.click(updateButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PROVIDER_CODEX_APP_SERVER_INSTALL_UPDATE_COMMAND,
        {},
        undefined,
      );
      expect(view.queryByRole("button", { name: /重试切换 Codex app-server/ }))
        .not.toBeInTheDocument();
    });
  });

  it.skip("keeps long Codex app-server update errors inside the update popover", async () => {
    const longReleaseNote =
      "LongStandaloneReleaseNoteWithNoNaturalBreaksForWindowsInstallersAndPowerShellErrorOutput";
    const longUpdateError =
      "Codex 安装器退出失败：Move-Item : Access to the path " +
      "'C:\\Users\\wangjunxue\\.codex\\packages\\standalone\\releases\\staging_0.142.2-x86_64-pc-windows-msvc.58144' " +
      "is denied. At line:845 char:13 + Move-Item -LiteralPath StagingDir-Destination ReleaseD ... " +
      "CategoryInfo : WriteError: (C:\\Users\\wangjunxue\\Downloads\\msvc.58144:DirectoryInfo) [Move-Item], IOException + FullyQualifiedErrorId : " +
      "MoveDirectoryItemIOError,Microsoft.PowerShell.Commands.MoveItemCommand";
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAppServerStatus({
      version: "codex-cli 0.142.0",
      latestVersion: "0.142.2",
      updateAvailable: true,
      updateState: "ready",
      preparedVersion: "0.142.2",
      releaseNotes: [longReleaseNote],
      updateError: longUpdateError,
    });

    const view = await renderFooter();
    await useConnectionStatus({ probe: false, loadBackend: false }).checkCodexAppServerUpdate();

    const updateButton = await view.findByRole("button", { name: /切换 Codex app-server/ });
    await fireEvent.mouseEnter(updateButton);

    const tooltip = await view.findByRole("tooltip");
    const error = view.getByText(longUpdateError);
    const releaseNote = view.getByText(longReleaseNote);
    expect(tooltip).toHaveTextContent(longUpdateError);
    expect(error).toBeInTheDocument();
    expect(releaseNote).toBeInTheDocument();
  });

  it.skip("shows Codex official account quota rings and hover details", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAccountQuotaStatus(officialQuota());

    const view = await renderFooter();
    const provider = providerBadge(view.container, "Codex · 官方账号");

    expect(invokeCount(QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND)).toBe(0);

    await fireEvent.mouseEnter(provider);

    await waitFor(() => {
      expect(invokeCount(QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND)).toBe(1);
    });

    const tooltip = await view.findByRole("tooltip");
    expect(tooltip).toHaveTextContent("重置次数可用 2 次");
    expect(tooltip).toHaveTextContent("连续 8 天");
  });

  it.skip("keeps the quota popover error state when the official quota call fails", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAccountQuotaStatus(officialQuota({
      available: false,
      fiveHour: null,
      weekly: null,
      sparkFiveHour: null,
      sparkWeekly: null,
      credits: null,
      sparkCredits: null,
      rateLimitResetCredits: null,
      accountUsage: null,
      usageError: null,
      error: "Codex 官方额度接口未返回可识别的额度数据。",
    }));

    const view = await renderFooter();
    const provider = providerBadge(view.container, "Codex · 官方账号");

    await fireEvent.mouseEnter(provider);

    expect(await view.findByRole("tooltip")).toHaveTextContent(
      "Codex 官方额度接口未返回可识别的额度数据。",
    );
    expect(view.queryByRole("button", { name: "登录" })).not.toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith(
      QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND,
      {},
      undefined,
    );
  });

  it.skip("shows a login action when the Codex official account is logged out", async () => {
    setMockActiveBackend("native-agentkit");
    setMockRouterMode("native-agentkit", "codex-account");
    setMockCodexAccountQuotaStatus(officialQuota({
      available: false,
      fiveHour: null,
      weekly: null,
      sparkFiveHour: null,
      sparkWeekly: null,
      credits: null,
      sparkCredits: null,
      rateLimitResetCredits: null,
      accountUsage: null,
      usageError: null,
      error: "Codex 未登录",
    }));

    const view = await renderFooter();
    const provider = providerBadge(view.container, "Codex · 官方账号");

    await fireEvent.mouseEnter(provider);

    const tooltip = await view.findByRole("tooltip");
    expect(tooltip).toHaveTextContent("Codex 未登录");
    const loginButton = view.getByRole("button", { name: "登录" });
    let resolveLogin!: () => void;
    mockInvoke.mockImplementationOnce(async () => {
      await new Promise<void>((resolve) => {
        resolveLogin = resolve;
      });
    });

    const clickPromise = fireEvent.click(loginButton);

    await waitFor(() => {
      expect(loginButton).toBeDisabled();
      expect(loginButton).toHaveTextContent("启动中...");
    });
    expect(mockInvoke).toHaveBeenCalledWith(
      PROVIDER_CODEX_ACCOUNT_START_LOGIN_COMMAND,
      {},
      undefined,
    );

    resolveLogin?.();
    await clickPromise;
    await waitFor(() => {
      expect(loginButton).toBeEnabled();
      expect(loginButton).toHaveTextContent("登录");
    });
  });
});

