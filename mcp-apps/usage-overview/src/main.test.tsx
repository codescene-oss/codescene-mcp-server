/* istanbul ignore file -- test code */
/* v8 ignore file */
import { cleanup, render, screen } from "@testing-library/react";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { useApp } = vi.hoisted(() => ({ useApp: vi.fn() }));
vi.mock("@modelcontextprotocol/ext-apps/react", () => ({ useApp }));

import { App, Dashboard, dashboardData, recentActivity } from "./main";

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
    score: 10,
    categories: ["Complex Method"],
    event_properties: { file_hash: "one", environment: "binary", quality_gates: "passed" },
  },
  {
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
      perfectScorePercent: 50,
      gatePassPercent: 50,
      categories: [
        { name: "Complex Method", count: 2 },
        { name: "Large Method", count: 1 },
      ],
    });
    expect(recentActivity([])).toMatchObject({ files: 0 });
  });
});

describe("dashboard", () => {
  it("renders lifetime and recent insights", () => {
    render(<Dashboard data={{ insights, outcomes }} />);
    expect(screen.getByText("120")).toBeTruthy();
    expect(screen.getByText("code-health-score")).toBeTruthy();
    expect(screen.getByText("1.4.7")).toBeTruthy();
    expect(screen.getByText("Average Code Health")).toBeTruthy();
    expect(screen.getByText("Complex Method")).toBeTruthy();
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
  });

  it("registers handlers and displays the dashboard", () => {
    let toolResult: unknown;
    useApp.mockImplementation((options) => {
      const app: Record<string, unknown> = {};
      options?.onAppCreated(app);
      toolResult = app.ontoolresult;
      return { app, error: null };
    });
    render(<App />);
    expect(screen.getByText("Your AI coding safety net")).toBeTruthy();
    expect(toolResult).toBeTypeOf("function");
  });
});
