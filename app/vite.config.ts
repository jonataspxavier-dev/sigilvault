import { defineConfig } from "vitest/config";

// Configuração do Vite para o Tauri (porta fixa 1420) e do Vitest.
// Vite config for Tauri (fixed port 1420) and Vitest.
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    // Duas páginas: a janela principal e o pop-up rápido. / Two pages: the main window and the quick popup.
    rollupOptions: { input: { main: "index.html", quick: "quick.html" } },
  },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
