import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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
  it("allows saving untouched when maximum response tokens already null", () => {
    const cfg = baseConfig();
    cfg.providers[0].num_predict = null;
    render(<ConfigForm accessToken="token" config={cfg} onSaved={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Save/i }));
    expect(screen.queryByText(/must be greater than zero/i)).not.toBeInTheDocument();
    const calls = (updateAdminConfig as unknown as ReturnType<typeof vi.fn>).mock.calls;
    require("fs").writeFileSync("/tmp/repro-out2.json", JSON.stringify(calls, null, 2));
  });

  it("new provider via Add provider keeps num_predict null when blank", () => {
    const cfg = baseConfig();
    render(<ConfigForm accessToken="token" config={cfg} onSaved={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: /Add provider/i }));

    // Expand the newly added provider (second one, "New provider")
    fireEvent.click(screen.getByRole("button", { name: /Remove New provider/i }).parentElement!);

    // Change type to OpenRouter
    const selects = screen.getAllByRole("combobox");
    require("fs").writeFileSync(
      "/tmp/selects.json",
      JSON.stringify(
        selects.map((el) => Array.from((el as HTMLSelectElement).options).map((o) => o.value)),
        null,
        2,
      ),
    );
    const typeSelect = selects.find((el) =>
      Array.from((el as HTMLSelectElement).options).some((o) => o.value === "open_router"),
    ) as HTMLSelectElement;
    fireEvent.change(typeSelect, { target: { value: "open_router" } });

    fireEvent.click(screen.getByRole("button", { name: /Save/i }));

    const calls = (updateAdminConfig as unknown as ReturnType<typeof vi.fn>).mock.calls;
    require("fs").writeFileSync("/tmp/repro-out3.json", JSON.stringify(calls, null, 2));
  });

  it("allows saving with maximum response tokens left blank", () => {
    render(<ConfigForm accessToken="token" config={baseConfig()} onSaved={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: /Remove OpenAI/i }).parentElement!);

    const input = screen.getByLabelText(/Maximum response tokens/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "" } });

    fireEvent.click(screen.getByRole("button", { name: /Save/i }));

    expect(screen.queryByText(/must be greater than zero/i)).not.toBeInTheDocument();
    const calls = (updateAdminConfig as unknown as ReturnType<typeof vi.fn>).mock.calls;
    require("fs").writeFileSync("/tmp/repro-out.json", JSON.stringify(calls, null, 2));
    require("fs").writeFileSync("/tmp/repro-body.html", document.body.innerHTML);
  });
});
