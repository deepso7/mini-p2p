import { defineConfig } from "oxfmt";
import ultracite from "ultracite/oxfmt";

export default defineConfig({
  ...ultracite,
  ignorePatterns: [
    "**/*",
    "!**/",
    "!bindings/ts/**",
    "!examples/react-native/**",
    "!docs/*.ts",
    "!/*.ts",
    "!/*.json",
    "!/*.yaml",
    ...(ultracite.ignorePatterns ?? []),
    "**/dist",
    "**/lib",
    "**/generated/**",
    "examples/react-native/android",
    "examples/react-native/ios",
    "bindings/ts/react-native/src/NativeMinip2p.ts",
    "bindings/ts/react-native/src/native.tsx",
  ],
});
