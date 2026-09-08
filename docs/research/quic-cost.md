# QUIC cost vs HTTPS-like one-shot

Investigation of whether minip2p's QUIC adapter can be tuned toward HTTPS loopback latency and memory without breaking Sans-I/O, sync, or no_std goals.

Pinned to `main` at `786f187` (post v0.5.3, including PR #123). Probes:

```bash
cargo bench -p minip2p-quic --bench cost_probe
cargo bench -p minip2p-rs --features quic --bench cost_probe
cargo bench -p minip2p-quic --bench idle_poll
```

Numbers below are from a Linux x86_64 cloud VM, rustc 1.91.0, release. They are not the M2 figures in the original comparison. Use the splits, not the absolute milliseconds, when comparing to HTTPS p50 0.49 ms on another host.

## Verdict

The ~2× loopback gap is not hiding in quiche send/recv buffering, pending write queues, or `drain_send_queue`. A warm QUIC transport handshake already sits next to the HTTPS establish+1-RTT number. What HTTPS does not do, minip2p does after the crypto handshake:

1. Stateless Retry (on by default).
2. Identify (required for `PeerReady`).
3. A second multistream-select round for the application stream.

Noise and Yamux are TCP-only. QUIC uses libp2p TLS 1.3 via BoringSSL and native QUIC streams. Chasing a QUIC rewrite will not close the gap.

Memory is a different story. Holding 200 idle QUIC connections on two transports costs about 59 kB/conn combined (both sides). One Endpoint per peer session costs about 136 kB per client, which matches the reported 124 kB cold first-touch. Most of that is Endpoint construction (64 KiB stream read buffer, BoringSSL context, swarm/Identify state), not extra bytes inside quiche.

No production patch in this change. The remaining adapter tweaks are real but they shave idle CPU or dual-stack construct time. They do not make one-shot look like HTTPS.

## Measured split

### Transport only (no Identify, no multistream-select)

| Work | p50 | notes |
| --- | ---: | --- |
| `Ed25519Keypair::generate` | 0.028 ms | |
| libp2p TLS cert (P-256 + X.509) | 0.423 ms | inside every `QuicTransport::new` |
| `QuicTransport::new` | 0.471 ms | cert + BoringSSL context + bind + 64 KiB heap buffer |
| handshake, Retry on | 0.542 ms | reused client transport |
| handshake, Retry off | 0.500 ms | `require_address_validation: false` |
| handshake + native 9-byte echo, Retry on | 0.619 ms | |
| handshake + native 9-byte echo, Retry off | 0.577 ms | |

Retry is ~40 µs on this loopback, not hundreds. Cert generation is ~0.42 ms and is paid again for every new Endpoint.

`size_of::<quiche::Connection>()` is 15,152 bytes. That is the struct, not RSS. Heap on top of it is the rest of per-connection memory.

### Endpoint (Identify + application stream)

Timed section is dial → `wait_peer_ready` (Identify complete) → optional 64-byte echo. Construction is outside the timer, matching a reused process with a new session.

| Work | p50 |
| --- | ---: |
| `Endpoint::bind_quic` | 0.480 ms |
| setup, Retry on | 0.855 ms |
| setup, Retry off | 0.753 ms |
| oneshot 64-byte echo, Retry on | 0.947 ms |
| oneshot 64-byte echo, Retry off | 0.805 ms |

Against the 0.542 ms transport handshake:

- Identify + its multistream-select: ~0.31 ms
- Application stream + 64-byte echo: ~0.09 ms
- Retry on the Endpoint path: ~0.14 ms (noisier than the transport-only 40 µs because the server thread is also running Identify)

Reported user number was QUIC oneshot ~1.04 ms. This VM's oneshot with Retry is 0.95 ms. Same shape.

### Idle poll after PR #123

PR #123 reused the stream read buffer, cached the bound address, and cut idle poll ~73% (64 conns: 48.7 µs → 13.3 µs on M2).

This VM, current main:

| Conns | idle `poll` |
| ---: | ---: |
| 1 | 1.43 µs |
| 64 | 11.4 µs |
| 256 | 42.4 µs |
| 512 | 157 µs |

About 0.3 µs per idle connection. Further cuts here help a relay with hundreds of quiet peers. They do not move one-shot latency.

### RSS

Transport pair, many QUIC connections, Retry off, 1-hour idle timeout:

| n | delta RSS | per conn combined |
| ---: | ---: | ---: |
| 50 | 3080 kB | ~61 kB |
| 200 | 11836 kB | ~59 kB |

That is both client and server in one process, so ~30 kB/side. The advertised 10 MB connection window is not preallocated.

One Endpoint per client against one shared server:

| clients | delta RSS | per client |
| ---: | ---: | ---: |
| 1 | 164 kB | 164 kB |
| 20 | 2756 kB | 137 kB |
| 50 | 6808 kB | 136 kB |

This is the 124 kB cold first-touch shape: a new Endpoint is a 64 KiB stream buffer, a BoringSSL context, a generated cert, mio, swarm, and one quiche connection.

## What PR #123 already took

- One reused `stream_read_buffer` (64 KiB heap) instead of allocating per readable stream.
- Bound socket address cached; UDP recv still uses a 64 KiB **stack** buffer in `poll`, which is fine.
- CID indexing without a full-table scan.
- Idle poll no longer walks work that PR #123 measured as dominant.

Still allocated on the packet path, and not worth a rewrite for one-shot:

- `drain_send_queue` collects stream keys into a `Vec` on every drain. Empty maps do not heap-allocate. Many streams on a busy connection would. Skip-if-empty is a few lines if idle CPU at 512 conns ever shows up in a profile.
- `poll_streams` runs `drain` + `flush` + GC on every established connection every `poll`, even when nothing was received. Same category.
- `StreamData` still `to_vec()`s the readable slice. That is the transport contract (owned event payloads), not a quiche mistake.

## Ranked opportunities

Impact is against the stated goals (oneshot latency toward 0.49 ms, idle ~42 kB/conn, cold ~124 kB). Effort and risk are adapter-local unless noted.

### 1. Stop treating Identify as part of one-shot  (high impact, high effort, API/risk)

`PeerReady` waits for Identify. HTTPS has no equivalent round trip. Skipping Identify, or making `open_stream` legal before `PeerReady` when the caller already knows the protocol list, is a swarm change. It is the only lever that can give back the ~0.31 ms.

Do not do this silently. NAT, relay, and DCUtR consume Identify addresses. A one-shot mode would have to say so.

### 2. Reuse an Endpoint  (high impact, no code, no risk)

Cold construct is 0.48 ms, almost entirely libp2p cert generation (0.42 ms). A process that builds one Endpoint per HTTP-like request pays that every time, plus 64 KiB + BoringSSL, which is the 136 kB/client figure.

HTTPS servers load a cert once. minip2p should too: one Endpoint, many `dial`s. Swarm is last-connection-wins per peer, so many concurrent peers need many PeerIds (many clients) or one server accepting many clients. They do not need many Endpoints on the client if the client talks to one server.

### 3. Turn off Retry for trusted listeners  (medium latency, already shipped)

`QuicLimits::require_address_validation` defaults to `true`. That is the right default on the public internet. On loopback or a private one-shot listener it adds a Retry RTT.

This VM: ~40 µs transport-only, ~140 µs on the Endpoint oneshot path. Not the 2×, but it is the only QUIC config knob that is visible in the oneshot number today.

Do not default it off. Amplification defense is why it exists.

### 4. Cache the generated cert on `QuicNodeConfig`  (low latency, small patch, low risk)

`build_quiche_config` calls `generate_certificate` on every `QuicTransport::new`. Dual-stack clones the config and generates twice. An `Arc<OnceLock<(Vec<u8>, Vec<u8>)>>` on the config would share one P-256 cert across those sockets.

Does not help a single `bind_quic`. Helps `dual_stack` construct (~0.42 ms saved). Fine follow-up, not the oneshot gap.

### 5. Shrink or knob the 64 KiB stream read buffer  (medium memory for cold Endpoint, small patch)

`STREAM_READ_BUFFER_SIZE` is 65,535 and lives for the lifetime of the transport. `stream_recv` already loops, so 8 KiB would be correct and would save ~56 kB per Endpoint. Cold 136 kB → ~80 kB. Still above HTTPS 42 kB.

Cost: a 1 MiB transfer becomes more `StreamData` events (owned `Vec`s). Either keep 64 KiB as default and add `QuicLimits::stream_read_buffer_size`, or pick 16 KiB and re-bench `e2e/quic/transfer_1mib`.

Does almost nothing for the 59 kB/conn many-connection hold, where the buffer is amortized.

### 6. Skip `poll_streams` when the connection got no packet  (low idle CPU, small patch, low risk)

After PR #123 this is ~0.3 µs/conn. Only interesting on a relay. Flag connections that received in this `poll` and skip the rest, still running `handle_timeout` / keepalive.

## What not to chase

- **A Tokio or async rewrite.** The driver is already packet-woken via `wait_for_input`. Handshake is not sleeping on the 1 ms timer roundup (`deadline_for_timeout` only affects quiche's loss/idle timers).
- **Noise XX / Yamux on QUIC.** They are not on this path. TCP pays them; QUIC pays libp2p TLS + native streams + Identify.
- **quiche itself, datagram caps, connection HashMaps.** Warm handshake is 0.50 ms with Retry off, next to HTTPS 0.49 ms on the other machine. Adapter maps are tiny next to a 15 kB `quiche::Connection`.
- **Hardcoded 10 MB / 1 MB flow-control windows for RSS.** 200 idle conns are ~30 kB/side. quiche is not reserving the advertised window. Shrinking windows is a throughput/NAT tradeoff with no measured RSS win here.
- **`drain_send_queue` key collection, pending write queues, as the oneshot story.** Empty-queue collect is a non-allocating iterator. Pending writes exist for flow control, not for a 9-byte echo.
- **Keepalive / 30 s idle timeout for one-shot.** One-shot connections close. The 15 s ping matters for NAT reservations (`NatConfig::reservation_keep_alive_interval_ms`), which is working as designed (issue #83).
- **Pretending a QUIC micro-optimization gets to 0.49 ms.** The missing 0.4–0.5 ms is Identify + app negotiation + Retry, sitting on a handshake that is already HTTPS-shaped.

## Config knobs that matter for one-shot or many short-lived peers

Already exist:

- `require_address_validation` (Retry RTT on inbound)
- `idle_timeout_ms` (30 s default, keepalive at half)
- `max_connections` / `max_streams_per_connection` / `max_pending_stream_bytes` / `max_pending_datagrams`

Missing, and the only QUIC-local memory knobs that would help one Endpoint per session:

- cert reuse on `QuicNodeConfig` (construct CPU)
- stream read buffer size (cold RSS)
- flow-control windows in `QuicLimits` (not shown to affect RSS; useful if a later bulk-transfer tune wants them)

## Recommended next steps

1. If the product is HTTP-like one-shot, decide whether Identify is required before the first application stream. That is a swarm/Endpoint design issue, not a quiche issue.
2. Document that one Endpoint per request is the expensive shape, and that Retry is optional for trusted listeners.
3. Optional small follow-ups, each with its own microbench: cert `OnceLock` on `QuicNodeConfig`; `stream_read_buffer_size` limit; idle `poll_streams` skip.

Re-run the probes on the HTTPS comparison host before treating any of these milliseconds as SLOs.
