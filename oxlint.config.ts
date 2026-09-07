import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import react from "ultracite/oxlint/react";

export default defineConfig({
  extends: [core, react],
  ignorePatterns: [
    "**/*",
    "!**/",
    "!bindings/ts/**",
    "!docs/*.ts",
    "!/*.ts",
    "!/*.json",
    "!/*.yaml",
    ...(core.ignorePatterns ?? []),
    "**/dist",
    "**/lib",
    "**/generated/**",
    "bindings/ts/react-native/example/android",
    "bindings/ts/react-native/example/ios",
    "bindings/ts/react-native/src/NativeMinip2p.ts",
    "bindings/ts/react-native/src/native.tsx",
  ],
  // Match ultracite's ESLint core (off). Oxlint 1.81 started failing
  // function-hoisted helpers used before their declarations in tests/scripts.
  rules: {
    "no-use-before-define": "off",
  },
});
