import { useApp, useHostStyles } from "@modelcontextprotocol/ext-apps/react";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

type Insights = Record<string, unknown>;
type Metric = { key: string; label: string; value: number };
type ToolUsage = { name: string; count: number };
type Outcome = {
  timestamp?: string;
  score?: number;
  categories?: string[];
  event_properties?: {
    file_hash?: string;
    environment?: string;
    quality_gates?: string;
  };
};
type RecentActivity = {
  events: number;
  files: number;
  averageScore?: number;
  perfectScorePercent?: number;
  gatePassPercent?: number;
  categories: ToolUsage[];
  environments: ToolUsage[];
};
type DashboardData = { insights: Insights; outcomes: Outcome[] };

const emptyInsights: Insights = {};
const nonMetricKeys = new Set(["user_identity", "number_of_installations"]);
const metricAliases = [
  { label: "Code Health uplifts", keys: ["number_of_uplifts"] },
  { label: "Declines prevented", keys: ["number_of_degradations_prevented"] },
];

function displayLabel(key: string): string {
  return key.replace(/^number-of-/, "").replaceAll(/[-_]/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function numericEntries(value: unknown, prefix = ""): Metric[] {
  if (!value || typeof value !== "object") return [];
  if (Array.isArray(value)) return [];
  return Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    if (nonMetricKeys.has(key)) return [];
    if (typeof child === "number") return [{ key: path, label: displayLabel(key), value: child }];
    return numericEntries(child, path);
  });
}

function featuredMetrics(insights: Insights): Metric[] {
  const all = numericEntries(insights);
  const featured = metricAliases.flatMap(({ label, keys }) => {
    const match = all.find(({ key }) => keys.some((alias) => key.endsWith(alias)));
    return match ? [{ ...match, label }] : [];
  });
  return featured;
}

function toolUsage(insights: Insights): ToolUsage[] {
  const summary = recordValue(insights.summary);
  const candidate = summary?.most_used_tools;
  if (Array.isArray(candidate)) return toolUsageFromArray(candidate);
  return [];
}

function recordValue(value: unknown): Insights | undefined {
  if (!value || typeof value !== "object") return undefined;
  if (Array.isArray(value)) return undefined;
  return value as Insights;
}

function toolUsageFromArray(items: unknown[]): ToolUsage[] {
  return items.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const value = item as Record<string, unknown>;
    const name = value.tool ?? value.name ?? value["tool-name"];
    const count = value.count ?? value.runs ?? value.uses;
    return typeof name === "string" && typeof count === "number" ? [{ name, count }] : [];
  }).sort((left, right) => right.count - left.count);
}

export function dashboardData(result: CallToolResult): DashboardData {
  const content = result.structuredContent as {
    insights?: Insights;
    recent_outcomes?: { outcomes?: Outcome[] };
  } | undefined;
  return {
    insights: content?.insights ?? emptyInsights,
    outcomes: content?.recent_outcomes?.outcomes ?? [],
  };
}

function countedValues(values: string[]): ToolUsage[] {
  const counts = new Map<string, number>();
  values.forEach((value) => counts.set(value, (counts.get(value) ?? 0) + 1));
  return [...counts].map(([name, count]) => ({ name, count })).sort((a, b) => b.count - a.count);
}

export function recentActivity(outcomes: Outcome[]): RecentActivity {
  const scored = outcomes.filter((outcome) => typeof outcome.score === "number");
  const gates = outcomes.flatMap((outcome) => outcome.event_properties?.quality_gates ?? []);
  const files = new Set(outcomes.flatMap((outcome) => outcome.event_properties?.file_hash ?? []));
  const scoreTotal = scored.reduce((total, outcome) => total + (outcome.score ?? 0), 0);
  return {
    events: outcomes.length,
    files: files.size,
    averageScore: scored.length ? scoreTotal / scored.length : undefined,
    perfectScorePercent: scored.length ? scored.filter(({ score }) => score === 10).length / scored.length * 100 : undefined,
    gatePassPercent: gates.length ? gates.filter((gate) => gate === "passed").length / gates.length * 100 : undefined,
    categories: countedValues(outcomes.flatMap(({ categories }) => categories ?? [])),
    environments: countedValues(outcomes.flatMap((outcome) => outcome.event_properties?.environment ?? [])),
  };
}

function MetricCards({ metrics }: { metrics: Metric[] }) {
  return (
    <section className="metrics">
      {metrics.map(({ key, label, value }, index) => (
        <article className={`metric metric-${index + 1}`} key={key}>
          <span>{label}</span>
          <strong>{value.toLocaleString()}</strong>
        </article>
      ))}
    </section>
  );
}

function ToolBars({ tools }: { tools: ToolUsage[] }) {
  const maximum = Math.max(...tools.map(({ count }) => count), 1);
  return (
    <section className="panel tools">
      <div className="panel-heading"><h2>Most used tools</h2><span>RUNS</span></div>
      {tools.length === 0 && <p className="empty">No tool breakdown reported.</p>}
      {tools.slice(0, 7).map(({ name, count }) => (
        <div className="tool-row" key={name}>
          <div><code>{name.replaceAll("_", " ")}</code><strong>{count.toLocaleString()}</strong></div>
          <div className="track"><i style={{ width: `${(count / maximum) * 100}%` }} /></div>
        </div>
      ))}
    </section>
  );
}

function RecentSnapshot({ outcomes }: { outcomes: Outcome[] }) {
  const activity = recentActivity(outcomes);
  const stats = [
    ["Files reviewed", activity.files.toLocaleString()],
    ["Average Code Health", activity.averageScore?.toFixed(2) ?? "-"],
    ["Perfect scores", activity.perfectScorePercent === undefined ? "-" : `${activity.perfectScorePercent.toFixed(0)}%`],
    ["Gate pass rate", activity.gatePassPercent === undefined ? "-" : `${activity.gatePassPercent.toFixed(0)}%`],
  ];
  return (
    <section className="panel recent">
      <div className="panel-heading"><h2>Scope of your recent work</h2><span>LATEST 100 EVENTS</span></div>
      <div className="recent-grid">
        <div className="snapshot-stats">
          {stats.map(([label, value]) => <div key={label}><span>{label}</span><strong>{value}</strong></div>)}
        </div>
        <div className="signal-list">
          <h3>Common findings</h3>
          <div className="chips">
            {activity.categories.slice(0, 5).map(({ name, count }) => <span key={name}>{name} <b>{count}</b></span>)}
          </div>
          <h3>Environments</h3>
          <div className="chips">
            {activity.environments.map(({ name, count }) => <span key={name}>{name} <b>{count}</b></span>)}
          </div>
        </div>
      </div>
    </section>
  );
}

export function Dashboard({ data }: { data: DashboardData }) {
  const { insights, outcomes } = data;
  const metrics = featuredMetrics(insights);
  return (
    <main>
      <header>
        <div>
          <p className="eyebrow">CODESCENE / MCP SAFEGUARDS</p>
          <h1>Your AI coding safety net</h1>
          <p className="subtitle">Current usage insights for you</p>
        </div>
        <div className="status"><i /> API CONNECTED</div>
      </header>
      <MetricCards metrics={metrics} />
      <RecentSnapshot outcomes={outcomes} />
      <ToolBars tools={toolUsage(insights)} />
    </main>
  );
}

export function App() {
  const [data, setData] = useState<DashboardData>({ insights: emptyInsights, outcomes: [] });
  const { app, error } = useApp({
    appInfo: { name: "CodeScene MCP usage", version: "0.2.0" },
    capabilities: {},
    onAppCreated: (createdApp) => {
      createdApp.ontoolresult = async (result) => setData(dashboardData(result));
      createdApp.onerror = console.error;
    },
  });
  useHostStyles(app, app?.getHostContext());

  if (error) return <main className="state">Unable to connect: {error.message}</main>;
  if (!app) return <main className="state">Connecting to host...</main>;
  return <Dashboard data={data} />;
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<StrictMode><App /></StrictMode>);
