//! Endpoint-layer cost probe: Identify-complete setup vs transport-only, plus RSS.
//!
//! Complements `minip2p-quic`'s transport probe. `setup` here is dial +
//! `wait_peer_ready`, which waits for Identify. `oneshot` then opens an
//! application stream and echoes 64 bytes.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use minip2p::{Endpoint, Event, PeerAddr, PeerId, QuicLimits, StreamId};

const ECHO: &str = "/minip2p/bench/echo/1";
const TIMEOUT: Duration = Duration::from_secs(10);
const SAMPLES: usize = 100;
const WARMUP: usize = 10;

fn main() {
    println!("minip2p endpoint QUIC cost probe  samples={SAMPLES}  warmup={WARMUP}");

    probe_construct();
    probe("setup/retry", true, false);
    probe("setup/no-retry", false, false);
    probe("oneshot-64b/retry", true, true);
    probe("oneshot-64b/no-retry", false, true);
    probe_rss_endpoints();
}

fn probe_construct() {
    let mut samples = Vec::with_capacity(SAMPLES);
    for i in 0..WARMUP + SAMPLES {
        let started = Instant::now();
        let endpoint = bind(true);
        let elapsed = started.elapsed();
        drop(endpoint);
        if i >= WARMUP {
            samples.push(elapsed);
        }
    }
    report("construct/Endpoint", &samples);
}

fn probe(label: &str, retry: bool, echo: bool) {
    let server = EchoServer::start(retry);
    let mut samples = Vec::with_capacity(SAMPLES);
    for i in 0..WARMUP + SAMPLES {
        let mut client = bind(retry);
        let started = Instant::now();
        client.dial(&server.address).expect("dial");
        client
            .wait_peer_ready(server.address.peer_id(), TIMEOUT)
            .expect("wait")
            .expect("ready timeout");
        if echo {
            do_echo(&mut client, server.address.peer_id());
        }
        let elapsed = started.elapsed();
        drop(client);
        if i >= WARMUP {
            samples.push(elapsed);
        }
    }
    drop(server);
    report(label, &samples);
}

fn do_echo(client: &mut Endpoint, peer: &PeerId) {
    let payload = [0x5a; 64];
    let stream = client.open_stream(peer, ECHO).expect("open");
    let deadline = Instant::now() + TIMEOUT;
    let mut sent = false;
    let mut received = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "echo timed out");
        let Some(event) = client.next_event(Duration::from_millis(50)).expect("poll") else {
            continue;
        };
        match event {
            Event::StreamReady {
                peer_id,
                stream_id,
                protocol_id,
                initiated_locally: true,
                ..
            } if peer_id == *peer && stream_id == stream && protocol_id == ECHO && !sent => {
                client
                    .send_stream(peer, stream, payload.to_vec())
                    .expect("send");
                sent = true;
            }
            Event::StreamData {
                peer_id,
                stream_id,
                data,
                ..
            } if peer_id == *peer && stream_id == stream => {
                received.extend_from_slice(&data);
                if received.len() == payload.len() {
                    assert_eq!(received, payload);
                    return;
                }
            }
            _ => {}
        }
    }
}

fn probe_rss_endpoints() {
    println!("rss: one Endpoint per client session against one shared server");
    for count in [1usize, 20, 50] {
        let (baseline, held, n) = rss_client_endpoints(count);
        let delta = held.saturating_sub(baseline);
        let per = delta / count as u64;
        println!(
            "  clients={count:<3}  baseline={baseline} kB  held={held} kB  delta={delta} kB  ~{per} kB/client  connected={n}"
        );
    }
}

fn rss_client_endpoints(count: usize) -> (u64, u64, usize) {
    let server = EchoServer::start(false);
    let baseline = rss_kb();
    let mut clients = Vec::with_capacity(count);
    for _ in 0..count {
        let mut client = bind(false);
        client.dial(&server.address).expect("dial");
        client
            .wait_peer_ready(server.address.peer_id(), TIMEOUT)
            .expect("wait")
            .expect("ready timeout");
        clients.push(client);
    }
    let held = rss_kb();
    let n = clients.len();
    drop(clients);
    drop(server);
    (baseline, held, n)
}

fn bind(retry: bool) -> Endpoint {
    Endpoint::builder()
        .agent_version("minip2p-cost-probe")
        .protocol(ECHO)
        .quic_limits(QuicLimits {
            require_address_validation: retry,
            idle_timeout_ms: 30_000,
            ..QuicLimits::default()
        })
        .bind_quic("127.0.0.1:0")
        .expect("bind")
}

struct EchoServer {
    address: PeerAddr,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl EchoServer {
    fn start(retry: bool) -> Self {
        let mut endpoint = bind(retry);
        let address = endpoint.listen().expect("listen");
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut streams = HashSet::<(PeerId, StreamId)>::new();
            while !worker_stop.load(Ordering::Relaxed) {
                let event = endpoint
                    .next_event(Duration::from_millis(20))
                    .expect("server poll");
                match event {
                    Some(Event::StreamReady {
                        peer_id,
                        stream_id,
                        protocol_id,
                        initiated_locally: false,
                        ..
                    }) if protocol_id == ECHO => {
                        streams.insert((peer_id, stream_id));
                    }
                    Some(Event::StreamData {
                        peer_id,
                        stream_id,
                        data,
                        ..
                    }) if streams.contains(&(peer_id.clone(), stream_id)) => {
                        endpoint
                            .send_stream(&peer_id, stream_id, data)
                            .expect("echo");
                    }
                    Some(Event::StreamClosed {
                        peer_id, stream_id, ..
                    }) => {
                        streams.remove(&(peer_id, stream_id));
                    }
                    _ => {}
                }
            }
        });
        Self {
            address,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for EchoServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("server");
        }
    }
}

fn report(label: &str, samples: &[Duration]) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let last = sorted.len().saturating_sub(1);
    let p = |hundredths: usize| {
        let idx = last.saturating_mul(hundredths) / 100;
        sorted.get(idx.min(last)).copied().expect("samples")
    };
    let sum: Duration = sorted.iter().copied().sum();
    let mean = sum / u32::try_from(sorted.len()).expect("sample count fits u32");
    println!(
        "{label:<24}  n={}  p50={}  p90={}  p99={}  mean={}  min={}  max={}",
        sorted.len(),
        fmt(p(50)),
        fmt(p(90)),
        fmt(p(99)),
        fmt(mean),
        fmt(sorted.first().copied().expect("samples")),
        fmt(sorted.last().copied().expect("samples")),
    );
}

fn fmt(d: Duration) -> String {
    format!("{:.3} ms", d.as_secs_f64() * 1_000.0)
}

fn rss_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }
    0
}
