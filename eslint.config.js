import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";

export default tseslint.config(
  {
    ignores: [
      "dist",
      "node_modules",
      "src-tauri",
      "site",
      "agentgate-promo",
      "provider-catalog",
      // tauri-specta 自动生成,不参与 lint
      "src/lib/bindings.ts",
    ],
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs["recommended-latest"].rules,
      ...reactRefresh.configs.vite.rules,
      // 首次引入 ESLint:以下为风格/性能建议类(非 bug),先降为 warning 渐进治理,
      // 不阻断 CI;no-unused-vars 等真实问题仍保持 error。
      "@typescript-eslint/no-explicit-any": "warn",
      "@typescript-eslint/no-empty-object-type": "warn",
      "react-hooks/set-state-in-effect": "warn",
      "react-hooks/refs": "warn",
      "react-hooks/purity": "warn",
      "react-refresh/only-export-components": "warn",
      // 允许下划线前缀表达"故意忽略"(如解构剥离 prop)
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    // 测试文件使用 vitest 全局变量
    files: ["src/**/*.test.{ts,tsx}", "vitest.setup.ts"],
    languageOptions: {
      globals: { ...globals.node },
    },
    rules: {
      // 测试里会故意构造常量假值(如 false && "x")验证函数行为
      "no-constant-binary-expression": "off",
    },
  }
);
