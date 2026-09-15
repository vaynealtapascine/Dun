import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    // Parser and grouping tests pin their own reference instant and time zone;
    // this keeps anything that forgets to do so deterministic.
    env: { TZ: "UTC" },
  },
});
