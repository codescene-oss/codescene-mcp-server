/* istanbul ignore file -- test code */
/* v8 ignore file */
// @vitest-environment node
import { describe, expect, it } from "vitest";

import viteConfig from "./vite.config";

describe("Vite configuration", () => {
  it("builds and covers the embedded app", () => {
    expect(viteConfig.build?.rollupOptions?.input).toBe("mcp-usage-overview.html");
    expect(viteConfig.test?.coverage?.reporter).toContain("cobertura");
  });
});
