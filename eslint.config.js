// Lint rules for the webview half of the app.
//
// This exists because the code was already relying on it: FloatingBar carries
// an `// eslint-disable-next-line react-hooks/exhaustive-deps`, which is a
// deliberate, load-bearing suppression of a rule that nothing was enforcing —
// so the suppression documented a decision no tool was checking.

import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri/target", "node_modules"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,

      // An unused parameter named `_` is how this codebase writes "this
      // argument exists to reach the next one", which is not an oversight.
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],

      // The Tauri boundary hands back `unknown` and errors arrive as `unknown`;
      // both get narrowed at the point of use rather than at the signature.
      "@typescript-eslint/no-explicit-any": "error",
    },
  },
  {
    // Node context, not browser: this file and the Vite config run on the host.
    files: ["*.config.{js,ts}"],
    languageOptions: { globals: globals.node },
  },
);
