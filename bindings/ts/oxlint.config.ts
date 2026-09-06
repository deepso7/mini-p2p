import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import react from "ultracite/oxlint/react";

export default defineConfig({
  extends: [core, react],
  ignorePatterns: [
    ...(core.ignorePatterns ?? []),
    "**/dist",
    "**/lib",
    "react-native/example/android",
    "react-native/example/ios",
    "react-native/src/NativeMinip2p.ts",
    "react-native/src/native.tsx",
  ],
  // Match ultracite's ESLint core (off). Oxlint 1.81 started failing
  // function-hoisted helpers used before their declarations in tests/scripts.
  rules: {
    "no-use-before-define": "off",
  },
});
