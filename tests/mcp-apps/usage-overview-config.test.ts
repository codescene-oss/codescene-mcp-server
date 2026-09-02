// @vitest-environment node
import { describe, expect, it } from "vitest";

import viteConfig from "../../mcp-apps/usage-overview/vite.config";

describe("Vite configuration", () => {
  it("builds the embedded app", () => {
    expect(viteConfig.build?.rollupOptions?.input).toBe("mcp-usage-overview.html");
  });
});
