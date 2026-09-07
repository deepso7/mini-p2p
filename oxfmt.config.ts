import { defineConfig } from "oxfmt";
import ultracite from "ultracite/oxfmt";

export default defineConfig({
  ...ultracite,
  ignorePatterns: [
    ...(ultracite.ignorePatterns ?? []),
    "**/dist",
    "**/lib",
    "bindings/ts/react-native/example/android",
    "bindings/ts/react-native/example/ios",
    "bindings/ts/react-native/src/NativeMinip2p.ts",
    "bindings/ts/react-native/src/native.tsx",
  ],
});
