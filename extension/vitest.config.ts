import { defineConfig } from "vitest/config";

// Testes de unidade no Node (WebCrypto e WebAssembly existem lá). O teste de ponta a ponta com o
// app real só roda com SIGILVAULT_E2E=1.
// Unit tests in Node (WebCrypto and WebAssembly exist there). The end-to-end test with the real
// app only runs with SIGILVAULT_E2E=1.
export default defineConfig({
  test: {
    environment: "node",
    include: process.env.SIGILVAULT_E2E ? ["src/**/*.e2e.test.ts"] : ["src/**/*.test.ts"],
    exclude: process.env.SIGILVAULT_E2E ? [] : ["src/**/*.e2e.test.ts"],
    testTimeout: process.env.SIGILVAULT_E2E ? 120_000 : 5_000,
  },
});
