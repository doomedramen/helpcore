import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ConfigForm from "./config-form";
import type { AdminConfig } from "@/lib/types";
import { updateAdminConfig } from "@/lib/api";

vi.mock("@/lib/api", () => ({
  updateAdminConfig: vi.fn(),
}));

function baseConfig(): AdminConfig {
  return {
    config_path: "/tmp/config.toml",
    config_writable: true,
    config_writability_error: null,
    server: { name: "Test", url: "http://localhost:3000", port: 3000 },
    logging_level: "info",
    registry_url: "https://example.com/plugins.json",
    plugin_blacklist: [],
    providers: [
      {
        id: "openai",
        name: "OpenAI",
        provider_type: "openai",
        api_key_configured: true,
        url: null,
        default_model: "gpt-4o",
        roles: ["chat"],
        num_ctx: 128000,
        num_predict: 8192,
      },
    ],
    sandbox: { enabled: true, image: "img", timeout: 30, memory_mb: 512, host: null },
    restart_required: false,
  };
}

describe("ConfigForm num_predict", () => {
  beforeEach(() => {
    vi.mocked(updateAdminConfig).mockReset();
    vi.mocked(updateAdminConfig).mockImplementation(async () => baseConfig());
  });

  it("allows saving untouched when maximum response tokens already null", async () => {
    const cfg = baseConfig();
    cfg.providers[0].num_predict = null;
    render(<ConfigForm accessToken="token" config={cfg} onSaved={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: /Save/i }));

    expect(screen.queryByText(/must be greater than zero/i)).not.toBeInTheDocument();
    await waitFor(() => expect(updateAdminConfig).toHaveBeenCalledOnce());
    expect(vi.mocked(updateAdminConfig).mock.calls[0][0].providers[0].num_predict).toBeNull();
  });

  it("new provider keeps num_predict null when the selected type defaults to blank", async () => {
    const cfg = baseConfig();
    render(<ConfigForm accessToken="token" config={cfg} onSaved={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: /Add provider/i }));

    const typeSelect = await screen.findByRole("combobox", { name: "Type" });
    fireEvent.change(typeSelect, { target: { value: "open_router" } });
    fireEvent.change(screen.getByLabelText("Default model"), {
      target: { value: "openai/gpt-4o" },
    });
    fireEvent.change(screen.getByLabelText(/^API key/), {
      target: { value: "test-key" },
    });

    fireEvent.click(screen.getByRole("button", { name: /Save/i }));

    await waitFor(() => expect(updateAdminConfig).toHaveBeenCalledOnce());
    expect(vi.mocked(updateAdminConfig).mock.calls[0][0].providers[1].num_predict).toBeNull();
  });

  it("allows saving with maximum response tokens left blank", async () => {
    render(<ConfigForm accessToken="token" config={baseConfig()} onSaved={() => {}} />);

    const providerHeading = screen.getByText("OpenAI");
    fireEvent.click(providerHeading.closest("button")!);

    const input = screen.getByLabelText(/Maximum response tokens/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "" } });

    fireEvent.click(screen.getByRole("button", { name: /Save/i }));

    expect(screen.queryByText(/must be greater than zero/i)).not.toBeInTheDocument();
    await waitFor(() => expect(updateAdminConfig).toHaveBeenCalledOnce());
    expect(vi.mocked(updateAdminConfig).mock.calls[0][0].providers[0].num_predict).toBeNull();
  });
});
