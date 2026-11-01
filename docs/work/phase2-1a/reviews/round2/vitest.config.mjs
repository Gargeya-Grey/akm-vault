import react from "file:///W:/API%20Keys%20Manager/desktop/node_modules/@vitejs/plugin-react/dist/index.js";

const base = "W:/API Keys Manager/desktop/node_modules";
export default {
  root: "C:/Users/Gargeya/AppData/Local/Temp/opencode/akm-unit1a-acceptance-round2",
  cacheDir: "C:/Users/Gargeya/AppData/Local/Temp/opencode/akm-unit1a-acceptance-round2/vite-cache",
  plugins: [react()],
  resolve: {
    alias: [
      { find: /^react\/jsx-dev-runtime$/, replacement: `${base}/react/jsx-dev-runtime.js` },
      { find: /^react\/jsx-runtime$/, replacement: `${base}/react/jsx-runtime.js` },
      { find: /^react$/, replacement: `${base}/react/index.js` },
      { find: /^@testing-library\/react$/, replacement: `${base}/@testing-library/react/dist/index.js` },
      { find: /^@tauri-apps\/api\/core$/, replacement: `${base}/@tauri-apps/api/core.js` },
      { find: /^@tauri-apps\/api\/event$/, replacement: `${base}/@tauri-apps/api/event.js` },
    ],
  },
  test: { environment: "jsdom", include: ["acceptance-handoff.test.tsx"], fileParallelism: false },
};
