import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import { AgentToolsSection } from "../AgentToolsSection";

vi.mock("../../../helpers", () => ({
  autoSaveConfig: vi.fn(),
  t: (key: string) => key,
}));
import { autoSaveConfig } from "../../../helpers";

function getRow(container: HTMLElement, toolName: string): HTMLElement | null {
  return container.querySelector(`[data-tool-name="${toolName}"]`);
}

function isToggleOn(row: HTMLElement | null): boolean {
  const btn = row?.querySelector(".hana-toggle");
  return !!btn?.classList.contains("on");
}

function clickToggle(row: HTMLElement | null) {
  const btn = row?.querySelector(".hana-toggle") as HTMLElement | null;
  if (btn) fireEvent.click(btn);
}

describe("AgentToolsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders registered optional toggles while ignoring global tools", () => {
    const { container } = render(
      <AgentToolsSection
        availableTools={["automation", "browser", "computer", "cron", "install_skill", "update_settings", "read"]}
        disabled={[]}
      />
    );
    expect(container.querySelectorAll("[data-tool-name]")).toHaveLength(5);
    expect(getRow(container, "automation")).toBeTruthy();
    expect(getRow(container, "browser")).toBeTruthy();
    expect(getRow(container, "computer")).toBeNull();
    expect(getRow(container, "cron")).toBeTruthy();
    expect(getRow(container, "install_skill")).toBeTruthy();
    expect(getRow(container, "update_settings")).toBeTruthy();
  });

  it("renders built-in optional toggles while availableTools is not returned yet", () => {
    const { container } = render(
      <AgentToolsSection
        availableTools={undefined as unknown as string[]}
        disabled={["update_settings"]}
      />
    );
    expect(container.querySelectorAll("[data-tool-name]")).toHaveLength(5);
    expect(getRow(container, "automation")).toBeTruthy();
    expect(getRow(container, "browser")).toBeTruthy();
    expect(getRow(container, "computer")).toBeNull();
  });

  it("hides optional tool when not in availableTools", () => {
    const { container } = render(
      <AgentToolsSection
        availableTools={["browser", "computer", "install_skill", "update_settings", "read"]}
        disabled={[]}
      />
    );
    expect(container.querySelectorAll("[data-tool-name]")).toHaveLength(3);
    expect(getRow(container, "browser")).toBeTruthy();
    expect(getRow(container, "computer")).toBeNull();
  });

  it("toggle shows ON when tool is not in disabled list", () => {
    const { container } = render(
      <AgentToolsSection availableTools={["browser"]} disabled={[]} />
    );
    expect(isToggleOn(getRow(container, "browser"))).toBe(true);
  });

  it("toggle shows OFF when tool is in disabled list", () => {
    const { container } = render(
      <AgentToolsSection availableTools={["browser"]} disabled={["browser"]} />
    );
    expect(isToggleOn(getRow(container, "browser"))).toBe(false);
  });

  it("clicking an ON toggle adds the tool to disabled list via autoSaveConfig", () => {
    const { container } = render(
      <AgentToolsSection availableTools={["browser", "cron"]} disabled={[]} />
    );
    clickToggle(getRow(container, "browser"));
    expect(autoSaveConfig).toHaveBeenCalledWith({
      tools: { disabled: ["browser"] },
    });
  });

  it("clicking an OFF toggle removes the tool from disabled list", () => {
    const { container } = render(
      <AgentToolsSection
        availableTools={["browser", "cron"]}
        disabled={["browser", "cron"]}
      />
    );
    clickToggle(getRow(container, "browser"));
    expect(autoSaveConfig).toHaveBeenCalledWith({
      tools: { disabled: ["cron"] },
    });
  });

  it("renders the section note and tool summaries", () => {
    const { container } = render(
      <AgentToolsSection availableTools={["browser"]} disabled={[]} />
    );
    expect(container.textContent).toContain("settings.agent.tools.description");
    expect(container.textContent).toContain("settings.agent.tools.items.browser.summary");
  });

  it("two rapid clicks on different toggles both reach autoSaveConfig", () => {
    const { container } = render(
      <AgentToolsSection availableTools={["browser", "cron"]} disabled={[]} />
    );
    clickToggle(getRow(container, "browser"));
    clickToggle(getRow(container, "cron"));

    expect(autoSaveConfig).toHaveBeenNthCalledWith(1, {
      tools: { disabled: ["browser"] },
    });
    expect(autoSaveConfig).toHaveBeenNthCalledWith(2, {
      tools: { disabled: ["browser", "cron"] },
    });
  });

  it("returns null when no optional tools are available", () => {
    const { container } = render(
      <AgentToolsSection availableTools={["read", "bash"]} disabled={[]} />
    );
    expect(container.querySelector(`.${"settings-section"}`)).toBeNull();
    expect(container.querySelectorAll("[data-tool-name]")).toHaveLength(0);
  });
});
