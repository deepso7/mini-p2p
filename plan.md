## Overview

A minimal, educational libp2p implementation designed for:

- **Learning**: Understanding how libp2p works under the hood
- **FFI-Friendly**: Exposed to TypeScript/JavaScript via WebAssembly
- **Sans-IO**: Pure state machines, host language handles all I/O
- **Browser-First**: Primary target is browser environment with WebSocket transport

## Architecture Principles

### 1. Sans-IO Design

All protocol logic is pure state machines. The library returns "I/O intents" that the host language executes:

```rust
// Rust: Returns what to do, doesn't do it
pub fn poll(&mut self) -> Vec<Action> {
    vec![
        Action::Dial { addr: "/ip4/1.2.3.4/tcp/4001".parse()? },
        Action::Send { conn_id: 1, data: message },
        Action::Wait { duration_ms: 5000 },
    ]
}
```

```typescript
// TypeScript: Executes the I/O
for (const action of swarm.poll()) {
  if (action.type === "DIAL") {
    const socket = new WebSocket(action.addr);
    // ... handle connection
  } else if (action.type === "SEND") {
    connections.get(action.conn_id).send(action.data);
  }
}
```

### 2. FFI-First API

All public APIs designed for WebAssembly FFI from the start:

- No Rust-specific types in public API (`String`, `Vec` converted to FFI-safe types)
- Reference counting for memory safety (`Arc<Mutex<T>>`)
- Callback-based JavaScript interop
- No panics across FFI boundaries

### 3. Educational Clarity

- Explicit state machines (no hidden async/await transformations)
- Extensive comments explaining "why" not just "what"
- Logging at every state transition
- Working examples for each milestone

## Project Structure

```
mini-p2p/
├── Cargo.toml                    # Rust workspace configuration
├── src/
│   ├── lib.rs                    # Library entry point, wasm_bindgen exports
│   ├── ffi/
│   │   ├── mod.rs                # FFI utilities and error handling
│   │   ├── callbacks.rs          # JavaScript callback types
│   │   └── types.rs              # FFI-safe type conversions
│   ├── core/
│   │   ├── mod.rs                # PeerId, Multiaddr, Keypair
│   │   ├── peer_id.rs            # Ed25519-based peer identity
│   │   ├── multiaddr.rs          # Address parsing and formatting
│   │   └── ffi.rs                # Core types FFI exports
│   ├── crypto/
│   │   ├── mod.rs                # Encryption abstractions
│   │   └── noise.rs              # Noise XX handshake protocol
│   ├── transport/
│   │   ├── mod.rs                # Transport trait definitions
│   │   ├── connection.rs         # Connection state management
│   │   └── ffi.rs                # Transport FFI bindings
│   ├── muxer/
│   │   └── mod.rs                # Simple stream multiplexing
│   ├── protocols/
│   │   ├── mod.rs                # Protocol handler traits
│   │   ├── multistream.rs        # Protocol negotiation (/multistream/1.0.0)
│   │   ├── ping.rs               # /ipfs/ping/1.0.0 implementation
│   │   ├── identify.rs           # /ipfs/id/1.0.0 implementation
│   │   ├── gossipsub/
│   │   │   ├── mod.rs            # GossipSub behavior
│   │   │   ├── mesh.rs           # Mesh maintenance logic
│   │   │   ├── mcache.rs         # Message cache (deduplication)
│   │   │   └── ffi.rs            # GossipSub FFI exports
│   │   └── ffi.rs                # Protocol handlers FFI
│   └── swarm/
│       ├── mod.rs                # Swarm state machine
│       ├── pool.rs               # Connection pool management
│       ├── event.rs              # Swarm event types
│       └── ffi.rs                # Swarm FFI exports
├── pkg/                          # wasm-pack output (auto-generated)
│   ├── mini_p2p.d.ts             # TypeScript type definitions
│   ├── mini_p2p.js               # JS glue code
│   └── mini_p2p_bg.wasm          # WebAssembly binary
└── examples/
    └── browser/                  # Browser examples
        ├── index.html            # Main demo page
        ├── index.js              # TypeScript entry point
        ├── swarm.js              # Swarm wrapper with callbacks
        ├── ui.js                 # UI handling
        ├── websocket.js          # WebSocket transport shim
        └── styles.css            # Styling
```

## Module Specifications

### 1. Core Types (`src/core/`)

#### PeerId

- **Purpose**: Cryptographic identity based on Ed25519
- **Properties**: 32-byte public key hash, base58-encoded string representation
- **FFI Methods**:
  - `generate() -> PeerId`: Create new random identity
  - `from_bytes(bytes: &[u8]) -> Result<PeerId, Error>`: Deserialize
  - `to_string() -> String`: Base58 encoding
  - `bytes() -> Vec<u8>`: Raw bytes

#### Multiaddr

- **Purpose**: Self-describing network addresses
- **Format**: `/protocol/value/protocol/value...` (e.g., `/ip4/1.2.3.4/tcp/4001`)
- **FFI Methods**:
  - `parse(addr: &str) -> Result<Multiaddr, Error>`: Parse from string
  - `to_string() -> String`: Format as string
  - `protocols() -> Vec<Protocol>`: Iterate over address components

### 2. Crypto Module (`src/crypto/`)

#### Noise XX Handshake

- **Pattern**: XX (sender and receiver transmit ephemeral and static keys)
- **States**:
  - `InitiatorStart`: Initial state
  - `InitiatorSentEphemeral`: Sent first message
  - `ResponderSentEphemeral`: Received first message, sent second
  - `InitiatorSentStatic`: Sent static key + auth
  - `Established`: Handshake complete, have cipher keys
- **FFI Methods**:
  - `initiate(prologue: &[u8]) -> NoiseState`: Start as initiator
  - `respond(prologue: &[u8]) -> NoiseState`: Start as responder
  - `write_message(payload: &[u8]) -> (NoiseState, Vec<u8>)`: Send message
  - `read_message(message: &[u8]) -> (NoiseState, Vec<u8>)`: Receive message
  - `is_established() -> bool`: Check if handshake complete
  - `into_cipher() -> (Cipher, Cipher)`: Get encryption/decryption ciphers

### 3. Transport Module (`src/transport/`)

#### Connection

- **Purpose**: Abstract a single connection (post-handshake)
- **Properties**:
  - `id: u64`: Unique connection identifier
  - `peer_id: Option<PeerId>`: None until authenticated via Noise
  - `addr: Multiaddr`: Remote address
- **FFI Methods**:
  - `id() -> u64`: Get connection ID
  - `peer_id() -> Option<PeerId>`: Get authenticated peer (if any)
  - `addr() -> String`: Get remote address

### 4. Protocols Module (`src/protocols/`)

#### Multistream-Select

- **Purpose**: Negotiate which protocol to use on a connection
- **Protocol**: `/multistream/1.0.0`
- **FFI Methods**:
  - `select(proposed: &[&str]) -> Result<String, Error>`: Client-side negotiation
  - `listen(supported: &[&str]) -> Result<String, Error>`: Server-side negotiation

#### Ping Protocol

- **Purpose**: Keepalive and latency measurement
- **Protocol ID**: `/ipfs/ping/1.0.0`
- **Handler States**:
  - `Idle`: Waiting to send ping
  - `WaitingPong`: Sent ping, waiting for response
  - `Cooldown`: Received pong, waiting before next ping
- **FFI Methods**:
  - `new(interval_ms: u64, timeout_ms: u64) -> PingHandler`: Create handler
  - `poll() -> Vec<PingAction>`: Get pending actions
  - `on_pong(payload: &[u8], latency_ms: u64)`: Receive pong response
  - `on_ping(payload: &[u8]) -> Vec<u8>`: Generate pong response
- **Events**:
  - `PingReceived { peer: PeerId, payload: Vec<u8> }`
  - `PongReceived { peer: PeerId, latency_ms: u64 }`
  - `PingTimeout { peer: PeerId }`

#### Identify Protocol

- **Purpose**: Exchange peer metadata (addresses, protocols supported)
- **Protocol ID**: `/ipfs/id/1.0.0`
- **FFI Methods**:
  - `new(public_key: &[u8], listen_addrs: &[String], protocols: &[&str]) -> IdentifyHandler`
  - `poll() -> Vec<IdentifyAction>`
  - `on_identify(info: IdentifyInfo) -> Vec<IdentifyEvent>`
- **Events**:
  - `Identified { peer: PeerId, info: IdentifyInfo }`

#### GossipSub Protocol

- **Purpose**: Publish/subscribe messaging with mesh topology
- **Protocol ID**: `/meshsub/1.1.0`
- **Components**:
  - **Mesh**: Tracks peers subscribed to each topic
  - **Message Cache**: LRU cache of seen message IDs (prevent loops)
  - **Heartbeat**: Periodic maintenance (graft/prune mesh peers)
  - **Scoring**: Peer reputation for gossip optimization
- **Handler States**:
  - `Subscribed`: Active subscription to topic
  - `Publishing`: Sending message to mesh
  - `Gossiping`: Forwarding received message
- **FFI Methods**:
  - `new(config: GossipSubConfig) -> GossipSubHandler`
  - `subscribe(topic: &str) -> Vec<GossipSubAction>`: Subscribe to topic
  - `unsubscribe(topic: &str) -> Vec<GossipSubAction>`: Unsubscribe
  - `publish(topic: &str, data: &[u8]) -> Vec<GossipSubAction>`: Publish message
  - `on_message(msg: GossipSubMessage) -> Vec<GossipSubEvent>`: Handle received message
  - `on_heartbeat() -> Vec<GossipSubAction>`: Periodic maintenance
- **Events**:
  - `Message { propagation_source: PeerId, message: GossipSubMessage }`
  - `Subscribed { peer: PeerId, topic: String }`
  - `Unsubscribed { peer: PeerId, topic: String }`
  - `GossipSubAction::Forward { message_id: String, topic: String, destinations: Vec<PeerId> }`

### 5. Swarm Module (`src/swarm/`)

#### Swarm

- **Purpose**: Central coordinator managing all connections and protocols
- **Properties**:
  - `connections: HashMap<u64, Connection>`: Active connections
  - `listeners: Vec<Listener>`: Listening transports
  - `pending_actions: VecDeque<Action>`: I/O actions to perform
  - `events: VecDeque<SwarmEvent>`: Events for host application
- **FFI Methods**:
  - `new(callbacks: Callbacks) -> Swarm`: Create swarm with JS callbacks
  - `poll() -> Vec<Action>`: Get pending I/O actions
  - `dial(addr: &str) -> Vec<Action>`: Initiate outbound connection
  - `listen(addr: &str) -> Vec<Action>`: Start listening
  - `on_connection_established(conn_id: u64, addr: &str) -> Vec<SwarmEvent>`: Connection opened
  - `on_connection_closed(conn_id: u64, reason: &str) -> Vec<SwarmEvent>`: Connection closed
  - `on_data_received(conn_id: u64, data: &[u8]) -> Vec<SwarmEvent>`: Data received
  - `on_data_sent(conn_id: u64, bytes_sent: usize) -> Vec<SwarmEvent>`: Send completed
  - `on_timer_expired(timer_id: u64) -> Vec<SwarmEvent>`: Timer fired
  - `broadcast(protocol: &str, data: &[u8]) -> Vec<Action>`: Send to all connections

#### SwarmEvent

Enumeration of all possible events:

- `ConnectionEstablished { peer_id: PeerId, addr: String }`
- `ConnectionClosed { peer_id: PeerId, reason: String }`
- `IncomingConnection { conn_id: u64, addr: String }`
- `ProtocolEvent { peer_id: PeerId, protocol: String, data: Vec<u8> }`
- `Ping(PingEvent)`
- `Identify(IdentifyEvent)`
- `GossipSub(GossipSubEvent)`

### 6. FFI Module (`src/ffi/`)

#### Callbacks

JavaScript callback interface:

```rust
pub struct Callbacks {
    on_dial: Closure<dyn Fn(String, u64)>,           // addr, pending_conn_id
    on_send: Closure<dyn Fn(u64, Box<[u8]>)>,        // conn_id, data
    on_close: Closure<dyn Fn(u64)>,                  // conn_id
    on_event: Closure<dyn Fn(String, JsValue)>,      // event_type, event_data
    on_listen: Closure<dyn Fn(String, u64)>,         // addr, listener_id
    on_timer: Closure<dyn Fn(u64, u64)>,             // timer_id, duration_ms
}
```

#### Type Conversions

- Rust `Vec<u8>` ↔ JavaScript `Uint8Array`
- Rust `String` ↔ JavaScript `string`
- Rust enums ↔ JavaScript objects with `type` field
- Rust `Option<T>` ↔ JavaScript `T | null`
- Rust `Result<T, E>` ↔ JavaScript `{ ok: T } | { err: string }`

## Action Types

The core of sans-IO. All I/O operations are represented as explicit actions:

```rust
pub enum Action {
    // Connection management
    Dial {
        pending_id: u64,
        addr: String,
    },
    Listen {
        addr: String,
    },
    Accept {
        listener_id: u64,
    },
    CloseConnection {
        conn_id: u64,
    },

    // I/O operations
    Send {
        conn_id: u64,
        data: Vec<u8>,
    },
    Receive {
        conn_id: u64,
        max_bytes: usize,
    },

    // Timers
    SetTimer {
        timer_id: u64,
        duration_ms: u64,
    },
    CancelTimer {
        timer_id: u64,
    },

    // Protocol-specific
    NegotiateProtocol {
        conn_id: u64,
        protocols: Vec<String>,
    },
    StartNoiseHandshake {
        conn_id: u64,
        role: HandshakeRole,  // Initiator or Responder
    },
}
```

## Event Types

Events flow from Rust to JavaScript via callbacks:

```rust
pub enum SwarmEvent {
    // Connection lifecycle
    ConnectionEstablished {
        conn_id: u64,
        peer_id: PeerId,
        addr: String,
    },
    ConnectionClosed {
        conn_id: u64,
        peer_id: Option<PeerId>,
        reason: String,
    },
    IncomingConnection {
        conn_id: u64,
        addr: String,
    },

    // Protocol events
    Ping(PingEvent),
    Identify(IdentifyEvent),
    GossipSub(GossipSubEvent),

    // Errors
    Error {
        conn_id: Option<u64>,
        error: String,
    },
}
```

## Data Types

### Binary Data

Raw bytes passed as `Uint8Array`:

```typescript
// Send binary
const data = new Uint8Array([0x01, 0x02, 0x03]);
swarm.publish("my-topic", data);

// Receive binary
swarm.onEvent = (type, event) => {
  if (type === "GOSSIPSUB_MESSAGE") {
    const bytes = new Uint8Array(event.data); // Raw bytes
    console.log("Received", bytes.length, "bytes");
  }
};
```

### UTF-8 Strings

Automatic encoding/decoding for string data:

```typescript
// Send string (auto-encoded to UTF-8)
swarm.publish("chat-room", "Hello, World!");

// Receive string (auto-decoded from UTF-8 if valid)
swarm.onEvent = (type, event) => {
  if (type === "GOSSIPSUB_MESSAGE") {
    const text = event.text; // String if valid UTF-8, null otherwise
    console.log("Message:", text);

    // Always have access to raw bytes too
    const bytes = event.data; // Uint8Array
  }
};
```

## Transport Implementations

### Browser WebSocket Transport

Primary transport for browser environment:

```typescript
// Browser handles WebSocket I/O
const ws = new WebSocket("wss://relay.example.com/p2p/QmPeerId");
ws.binaryType = "arraybuffer";

ws.onmessage = (event) => {
  const data = new Uint8Array(event.data);
  swarm.onDataReceived(connId, data);
};
```

### WebRTC DataChannel (Future)

Direct browser-to-browser connections:

- Uses public STUN/TURN servers for NAT traversal
- Requires signaling server for initial handshake
- Implemented after WebSocket transport is stable

## Public Relay Strategy

For browser-to-browser connectivity without dedicated infrastructure:

**Primary Relay**: [libp2p-webrtc-star](https://github.com/libp2p/js-libp2p-webrtc-star) or similar

- Public WebSocket relay servers
- Circuit relay v2 protocol for NAT traversal
- Free tier suitable for development/testing

**Fallback**: WebRTC with public STUN

- `stun.l.google.com:19302` (Google's public STUN)
- `stun1.l.google.com:19302`
- TURN server optional (for symmetric NAT)

## Example Usage

### Browser Chat Application

```typescript
import init, { Swarm, PeerId, Callbacks } from "./pkg/mini_p2p.js";

// Initialize WASM
await init();

// Create peer identity
const myPeerId = PeerId.generate();
console.log("My ID:", myPeerId.toString());

// Setup callbacks for I/O operations
const callbacks = new Callbacks(
  // onDial: Create WebSocket connection
  (addr, pendingId) => {
    const ws = new WebSocket(addr);
    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
      connections.set(pendingId, ws);
      swarm.onConnectionEstablished(pendingId, addr);
    };

    ws.onmessage = (event) => {
      const data = new Uint8Array(event.data);
      const events = swarm.onDataReceived(pendingId, data);
      handleEvents(events);
    };

    ws.onclose = () => {
      connections.delete(pendingId);
      swarm.onConnectionClosed(pendingId, "WebSocket closed");
    };
  },

  // onSend: Send data via WebSocket
  (connId, data) => {
    const ws = connections.get(connId);
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(data);
    }
  },

  // onClose: Close WebSocket
  (connId) => {
    const ws = connections.get(connId);
    if (ws) {
      ws.close();
      connections.delete(connId);
    }
  },

  // onEvent: Handle events from swarm
  (eventType, eventData) => {
    handleEvent(eventType, eventData);
  },

  // onListen: Start listening (not used in browser relay mode)
  (addr, listenerId) => {
    console.log("Listen requested on", addr);
  },

  // onTimer: Set timeout
  (timerId, durationMs) => {
    setTimeout(() => {
      swarm.onTimerExpired(timerId);
    }, durationMs);
  },
);

// Create swarm
const swarm = new Swarm(callbacks);

// Subscribe to chat topic
const actions = swarm.subscribe("chat-room");
executeActions(actions);

// Connect to relay
const relayAddr = "wss://relay.libp2p.io/p2p/QmRelayPeerId";
const dialActions = swarm.dial(relayAddr);
executeActions(dialActions);

// Send message
document.getElementById("send").onclick = () => {
  const message = document.getElementById("message").value;
  const publishActions = swarm.publish("chat-room", message);
  executeActions(publishActions);
};

// Handle events
function handleEvent(type, data) {
  switch (type) {
    case "CONNECTION_ESTABLISHED":
      console.log("Connected to", data.peer_id);
      break;
    case "GOSSIPSUB_MESSAGE":
      displayMessage(data.source, data.data);
      break;
    case "PING_PONG":
      console.log("Latency:", data.latency_ms, "ms");
      break;
  }
}

// Execute all pending actions
function executeActions(actions) {
  for (const action of actions) {
    // Handled by callbacks above
  }
}

// Main event loop (not needed with callback architecture)
// All events come through onEvent callback
```

## Dependencies

### Rust Dependencies

```toml
[dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
web-sys = "0.3"
snow = "0.9"                    # Noise protocol
ed25519-dalek = "2.0"           # Ed25519 signatures
rand = "0.8"                    # Random number generation
sha2 = "0.10"                   # SHA-256 hashing
bs58 = "0.5"                    # Base58 encoding
thiserror = "1.0"               # Error handling
log = "0.4"                     # Logging

[dependencies.web-sys]
version = "0.3"
features = [
    "console",
    "Window",
    "Document",
]
```

### Build Configuration

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[profile.release]
opt-level = "s"      # Optimize for size
lto = true           # Link-time optimization
```

## Testing Strategy

### Rust Unit Tests

Each module has comprehensive unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_generation() {
        let peer_id = PeerId::generate();
        assert_eq!(peer_id.bytes().len(), 32);
    }

    #[test]
    fn test_noise_handshake() {
        let mut initiator = NoiseState::initiate(b"prologue");
        let mut responder = NoiseState::respond(b"prologue");

        // Complete handshake
        let (initiator, msg1) = initiator.write_message(b"");
        let (responder, msg2) = responder.read_message(&msg1);
        // ... continue handshake

        assert!(initiator.is_established());
        assert!(responder.is_established());
    }
}
```

### Browser Integration Tests

Manual testing via example pages:

1. Open two browser tabs
2. Each generates unique PeerId
3. Connect both to relay
4. Exchange ping messages
5. Subscribe to topic and exchange messages

### FFI Safety Tests

Ensure no panics across FFI:

```rust
#[test]
fn test_ffi_no_panic() {
    let result = std::panic::catch_unwind(|| {
        let peer_id = PeerId::generate();
        let _ = peer_id.to_string();
    });
    assert!(result.is_ok());
}
```

## Success Criteria

### Milestone 1: Raw Connectivity

- [ ] Two browser tabs connect via WebSocket relay
- [ ] Exchange raw bytes (echo test)
- [ ] No crashes or memory leaks

### Milestone 2: Encrypted Connections

- [ ] Complete Noise XX handshake
- [ ] All subsequent traffic encrypted
- [ ] PeerId verified via handshake

### Milestone 3: Ping Protocol

- [ ] Automatic ping every 30 seconds
- [ ] Latency measurement accurate
- [ ] Timeout detection works

### Milestone 4: Identify Protocol

- [ ] Exchange listen addresses
- [ ] Exchange supported protocols
- [ ] Store peer metadata

### Milestone 5: GossipSub

- [ ] Subscribe to topic
- [ ] Publish message to topic
- [ ] Message received by all subscribers
- [ ] Mesh topology forms automatically
- [ ] Message deduplication works

### Milestone 6: Polish

- [ ] Comprehensive documentation
- [ ] Working chat application example
- [ ] Clear error messages
- [ ] Performance benchmarks

## Learning Goals

By completing this implementation, you will understand:

1. **Peer Identity**: How Ed25519 keys create cryptographic identities
2. **Transport Security**: How Noise protocol establishes encrypted channels
3. **Protocol Negotiation**: How multistream-select enables protocol multiplexing
4. **Connection Management**: How libp2p manages multiple concurrent connections
5. **PubSub Semantics**: How GossipSub efficiently propagates messages in a mesh
6. **State Machine Design**: How to build complex network protocols as explicit state machines
7. **FFI Design**: How to expose Rust libraries to JavaScript via WebAssembly
8. **Sans-IO Architecture**: How to separate protocol logic from I/O operations

## Future Extensions

After core implementation is complete:

- **QUIC Transport**: HTTP/3-based transport for better performance
- **WebRTC**: Direct browser-to-browser connections
- **Kademlia DHT**: Distributed hash table for peer discovery
- **Bitswap**: Content-addressed data exchange
- **Circuit Relay**: NAT traversal via relay servers
- **AutoNAT**: Automatic NAT detection and hole punching
