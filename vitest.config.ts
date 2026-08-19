import { defineConfig } from "vitest/config";

// 纯逻辑测试段配置：node 环境（无需 jsdom），零注册自动发现 test/segments/**/*.test.ts
export default defineConfig({
  test: {
    environment: "node",
    include: ["test/segments/**/*.test.ts"],
  },
});