# minip2p TypeScript

TypeScript bindings for [minip2p](https://minip2p.com).

| Package | Purpose |
| --- | --- |
| [`@minip2p/core`](./core) | Platform-neutral SDK, types, events, and backend contract |
| [`@minip2p/react-native`](./react-native) | Published React Native adapter with Android and iOS libraries |
| [`@minip2p/node`](./node) | Node.js adapter over the napi-rs binding shell |

The platform binary packages live in [`node-platforms`](./node-platforms). The Expo application lives in [`examples/react-native`](../../examples/react-native).

Platform adapters implement `@minip2p/core/backend` and re-export the public SDK types. See [FEATURES.md](./FEATURES.md) for the Rust-to-TypeScript capability map.

Run from the repository root. Docs and bindings share one pnpm workspace and lockfile.

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm --filter @minip2p/node native:build
pnpm test
pnpm lint
pnpm build
```
