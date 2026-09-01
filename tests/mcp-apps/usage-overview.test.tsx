import { cleanup, render, screen } from "@testing-library/react";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { useApp, useHostStyles } = vi.hoisted(() => ({ useApp: vi.fn(), useHostStyles: vi.fn() }));
vi.mock("@modelcontextprotocol/ext-apps/react", () => ({ useApp, useHostStyles }));

import { App, Dashboard, dashboardData, recentActivity } from "../../mcp-apps/usage-overview/src/main";

afterEach(cleanup);

const insights = {
  summary: {
    number_of_safeguard_checks: 120,
    number_of_degradations_prevented: 8,
    number_of_uplifts: 12,
    number_of_active_installations: 4,
    number_of_installations: 9,
    most_used_tools: [
      { tool: "code-health-review", count: 7 },
      { tool: "code-health-score", count: 20 },
    ],
    versions: [
      { version: "1.4.7", number_of_active_installs: 3 },
      { version: "1.3.7", number_of_active_installs: 0 },
    ],
  },
};

const outcomes = [
  {
    timestamp: "2026-08-02T10:00:00Z",
    score: 10,
    categories: ["Complex Method"],
    event_properties: { file_hash: "one", environment: "binary", quality_gates: "passed" },
  },
  {
    timestamp: "2026-08-01T10:00:00Z",
    score: 8,
    categories: ["Complex Method", "Large Method"],
    event_properties: { file_hash: "two", environment: "cs-agent", quality_gates: "failed" },
  },
];

describe("usage data", () => {
  it("extracts structured tool content", () => {
    const result = dashboardData({
      content: [],
      structuredContent: { insights, recent_outcomes: { outcomes } },
    } as CallToolResult);
    expect(result).toEqual({ insights, outcomes });
  });

  it("defaults missing structured content", () => {
    expect(dashboardData({ content: [] } as CallToolResult)).toEqual({ insights: {}, outcomes: [] });
  });

  it("aggregates the recent sample", () => {
    expect(recentActivity(outcomes)).toMatchObject({
      events: 2,
      files: 2,
      averageScore: 9,
      scoreTrend: "improving",
      perfectScorePercent: 50,
      gatePassPercent: 50,
      categories: [
        { name: "Complex Method", count: 2 },
        { name: "Large Method", count: 1 },
      ],
    });
    expect(recentActivity([])).toMatchObject({ files: 0 });
    expect(recentActivity([
      { timestamp: "2026-08-01T10:00:00Z", score: 9 },
      { timestamp: "2026-08-02T10:00:00Z", score: 7 },
    ])).toMatchObject({ scoreTrend: "declining" });
    expect(recentActivity([
      { timestamp: "2026-08-01T10:00:00Z", score: 8 },
      { timestamp: "2026-08-02T10:00:00Z", score: 8 },
    ])).toMatchObject({ scoreTrend: "stable" });
  });
});

describe("dashboard", () => {
  it("renders lifetime and recent insights", () => {
    render(<Dashboard data={{ insights, outcomes }} />);
    const cards = screen.getByRole("main").querySelectorAll(".metric span");
    expect([...cards].map((card) => card.textContent)).toEqual(["Code Health uplifts", "Declines prevented"]);
    expect(screen.getByText("code-health-score")).toBeTruthy();
    expect(screen.getByText("Scope of your recent work")).toBeTruthy();
    expect(screen.getByText("Average Code Health")).toBeTruthy();
    expect(screen.getByLabelText("Average Code Health is improving")).toBeTruthy();
    expect(screen.getByText("Complex Method")).toBeTruthy();
    expect(screen.getByRole("button", { name: "About Code Health uplifts" }).getAttribute("data-tooltip")).toBe(
      "Each uplift is a higher Code Health score than the preceding analysis of the same file.",
    );
    expect(screen.getByRole("button", { name: "About Declines prevented" }).getAttribute("data-tooltip")).toBe(
      "Counts files whose first two recorded scores declined, plus safeguard runs that reported one or more degraded files.",
    );
    expect(screen.queryByText("Safeguard checks")).toBeNull();
    expect(screen.queryByText("Active installations")).toBeNull();
    expect(screen.queryByText("Active versions")).toBeNull();
    expect(screen.queryByText("number_of_installations")).toBeNull();
  });

  it("renders empty tool and activity states", () => {
    render(<Dashboard data={{ insights: {}, outcomes: [] }} />);
    expect(screen.getByText("No tool breakdown reported.")).toBeTruthy();
    expect(screen.getAllByText("-")).toHaveLength(3);
  });
});

describe("app lifecycle", () => {
  beforeEach(() => useApp.mockReset());

  it("shows connection and error states", () => {
    useApp.mockReturnValueOnce({ app: null, error: null });
    const { rerender } = render(<App />);
    expect(screen.getByText("Connecting to host...")).toBeTruthy();
    useApp.mockReturnValueOnce({ app: null, error: new Error("broken") });
    rerender(<App />);
    expect(screen.getByText("Unable to connect: broken")).toBeTruthy();
    expect(screen.getByText("API ERROR")).toBeTruthy();
  });

  it("registers handlers and displays the dashboard", () => {
    let toolResult: unknown;
    useApp.mockImplementation((options) => {
      const app: Record<string, unknown> = { getHostContext: () => ({ theme: "dark" }) };
      options?.onAppCreated(app);
      toolResult = app.ontoolresult;
      return { app, error: null };
    });
    render(<App />);
    expect(screen.getByText("Your AI coding safety net")).toBeTruthy();
    expect(toolResult).toBeTypeOf("function");
    expect(useHostStyles).toHaveBeenCalledWith(expect.anything(), { theme: "dark" });
  });
});
