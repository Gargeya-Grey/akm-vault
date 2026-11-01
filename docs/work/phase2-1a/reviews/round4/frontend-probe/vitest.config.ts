import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  cacheDir: "./scratch-vite-cache",
  test: { environment: "jsdom", fileParallelism: false },
});
