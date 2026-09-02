import path from "node:path";

const repositoryRoot = path.resolve(import.meta.dirname, "../..");

export default {
  root: repositoryRoot,
  cacheDir: "mcp-apps/usage-overview/.cache/vitest",
  esbuild: {
    jsx: "automatic",
  },
  resolve: {
    alias: {
      "@modelcontextprotocol/ext-apps/react": path.resolve(
        repositoryRoot,
        "mcp-apps/usage-overview/node_modules/@modelcontextprotocol/ext-apps/dist/src/react/index.js",
      ),
      "@testing-library/react": path.resolve(
        repositoryRoot,
        "mcp-apps/usage-overview/node_modules/@testing-library/react/dist/index.js",
      ),
      "react/jsx-dev-runtime": path.resolve(
        repositoryRoot,
        "mcp-apps/usage-overview/node_modules/react/jsx-dev-runtime.js",
      ),
      "react/jsx-runtime": path.resolve(
        repositoryRoot,
        "mcp-apps/usage-overview/node_modules/react/jsx-runtime.js",
      ),
    },
  },
  test: {
    environment: "jsdom",
    include: ["tests/mcp-apps/usage-overview*.test.ts", "tests/mcp-apps/usage-overview*.test.tsx"],
    coverage: {
      provider: "v8",
      reporter: ["text", "cobertura"],
      reportsDirectory: "mcp-apps/usage-overview/coverage",
      include: ["mcp-apps/usage-overview/src/main.tsx"],
    },
  },
};
