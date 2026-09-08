# React Native example

An Expo development build using the local `@minip2p/react-native` workspace package. Expo Go cannot load its native library.

Run from the repository root:

```sh
pnpm install --frozen-lockfile
pnpm example prebuild
pnpm example android
# On macOS:
pnpm example ios
```

Build the native library with `pnpm rn:android` or `pnpm rn:ios` before building the app. See the [binding contribution guide](../../bindings/ts/react-native/CONTRIBUTING.md) for toolchain requirements.

`pnpm example clean` removes native build output. The generated `android/` and `ios/` projects are ignored and can be recreated with `pnpm example prebuild`.
