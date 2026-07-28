import { fireEvent, render, waitFor } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import NativeCredentialSection from "../src/pages/settings/NativeCredentialSection.vue";
import {
  NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND,
  NATIVE_CREDENTIAL_LOGIN_COMMAND,
  NATIVE_CREDENTIAL_PROVIDERS_COMMAND,
} from "../src/services/nativeAgent";
import { mockInvoke } from "./tauriMock";

describe("NativeCredentialSection", () => {
  it("加载凭据诊断并可通过登录保存 API Key", async () => {
    const view = render(NativeCredentialSection);

    await waitFor(() => {
      expect(view.getByRole("heading", { level: 2, name: "凭据" })).toBeInTheDocument();
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_CREDENTIAL_PROVIDERS_COMMAND)).toBe(true);
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND)).toBe(true);
      expect(view.getByText("Broker")).toBeInTheDocument();
      expect(view.getByText("未绑定")).toBeInTheDocument();
    });

    expect(view.getByText("尚未保存可用凭据。")).toBeInTheDocument();

    const providerSelect = view.container.querySelector(
      '[data-agent-id="settings.credentials.provider"]',
    ) as HTMLSelectElement;
    await fireEvent.update(providerSelect, "mutsuki.credential.openai");
    await fireEvent.update(
      view.getByPlaceholderText("粘贴 Console API Key"),
      "sk-test-not-a-real-secret",
    );
    await fireEvent.click(
      view.container.querySelector('[data-agent-id="settings.credentials.submit"]') as HTMLElement,
    );

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_CREDENTIAL_LOGIN_COMMAND)).toBe(true);
      expect(view.queryByText("尚未保存可用凭据。")).not.toBeInTheDocument();
      expect(view.getByRole("button", { name: "撤销凭据" })).toBeInTheDocument();
    });

    const loginCall = [...mockInvoke.mock.calls]
      .reverse()
      .find(([cmd]) => cmd === NATIVE_CREDENTIAL_LOGIN_COMMAND);
    expect(loginCall?.[1]).toEqual({
      input: expect.objectContaining({
        providerId: "mutsuki.credential.openai",
        kind: "api_key",
        secretMaterial: "sk-test-not-a-real-secret",
        source: "settings-login",
      }),
    });
  });

  it("导入模式切换文案并调用 import 命令", async () => {
    const view = render(NativeCredentialSection);

    await waitFor(() => {
      expect(view.getByRole("heading", { level: 2, name: /登录/ })).toBeInTheDocument();
    });

    await fireEvent.click(
      view.container.querySelector('[data-agent-id="settings.credentials.mode.import"]') as HTMLElement,
    );
    expect(view.getByRole("heading", { level: 2, name: "导入官方生成 Key" })).toBeInTheDocument();

    await fireEvent.update(
      view.getByPlaceholderText("粘贴官方流程生成的 Key"),
      "sk-import-not-a-real-secret",
    );
    await fireEvent.click(
      view.container.querySelector('[data-agent-id="settings.credentials.submit"]') as HTMLElement,
    );

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "native_credential_import")).toBe(true);
    });
  });
});
