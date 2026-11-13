import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // A loaded run with one worker can take seconds for the first render of a file (U2-C-02).
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
