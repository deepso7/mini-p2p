//! One-shot QUIC cost probe: handshake latency, Identify-free echo, and RSS.
//!
//! This is an investigation harness, not a Criterion suite. It drives the
//! socket with `wait_for_input` the same way a production host does, so the
//! numbers are comparable to Endpoint loopback rather than the 5 ms sleep
//! used by the QUIC integration tests.

use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use minip2p_core::PeerAddr;
use minip2p_platform::{Clock, StdClock};
use minip2p_quic::{QuicLimits, QuicNodeConfig, QuicTransport};
use minip2p_transport::{BlockingTransport, Transport, TransportEvent, WaitHandle};

const SAMPLES: usize = 200;
const WARMUP: usize = 20;
const ECHO: &[u8] = b"tiny-echo";
const WAIT: Duration = Duration::from_millis(50);

fn main() {
    println!("minip2p-quic cost probe  samples={SAMPLES}  warmup={WARMUP}");
    print_sizes();

    probe_construct();
    probe_construct_parts();
    probe_handshake("handshake/retry", default_limits(true));
    probe_handshake("handshake/no-retry", default_limits(false));
    probe_echo("echo-64b/retry", default_limits(true));
    probe_echo("echo-64b/no-retry", default_limits(false));
    probe_rss();
}

fn default_limits(retry: bool) -> QuicLimits {
    QuicLimits {
        require_address_validation: retry,
        idle_timeout_ms: 30_000,
        ..QuicLimits::default()
    }
}

fn print_sizes() {
    println!(
        "sizeof QuicTransport={}  QuicConnection is not public; quiche::Connection={}",
        size_of::<QuicTransport>(),
        size_of::<quiche::Connection>(),
    );
    println!(
        "sizeof quiche::Config={}  QuicNodeConfig={}",
        size_of::<quiche::Config>(),
        size_of::<QuicNodeConfig>(),
    );
}

fn probe_construct() {
    let mut samples = Vec::with_capacity(SAMPLES);
    for i in 0..WARMUP + SAMPLES {
        let started = Instant::now();
        let transport = QuicTransport::new(QuicNodeConfig::generate(), "127.0.0.1:0")
            .expect("construct transport");
        let elapsed = started.elapsed();
        drop(transport);
        if i >= WARMUP {
            samples.push(elapsed);
        }
    }
    report("construct/QuicTransport", &samples);
}

fn probe_construct_parts() {
    use minip2p_identity::Ed25519Keypair;
    let mut keygen = Vec::with_capacity(SAMPLES);
    let mut certs = Vec::with_capacity(SAMPLES);
    let mut bind = Vec::with_capacity(SAMPLES);
    for i in 0..WARMUP + SAMPLES {
        let started = Instant::now();
        let keypair = Ed25519Keypair::generate();
        let after_key = started.elapsed();
        let _ = minip2p_tls::generate_certificate(&keypair).expect("cert");
        let after_cert = started.elapsed();
        let transport =
            QuicTransport::new(QuicNodeConfig::new(keypair), "127.0.0.1:0").expect("bind");
        let after_new = started.elapsed();
        drop(transport);
        if i >= WARMUP {
            keygen.push(after_key);
            certs.push(after_cert.saturating_sub(after_key));
            bind.push(after_new.saturating_sub(after_cert));
        }
    }
    report("construct/keypair", &keygen);
    report("construct/libp2p-cert", &certs);
    report("construct/new(existing-key)", &bind);
}

fn probe_handshake(label: &str, limits: QuicLimits) {
    let server = Server::spawn(limits.clone(), Mode::Handshake);
    let mut samples = Vec::with_capacity(SAMPLES);
    let mut client = bind_client(limits);
    for i in 0..WARMUP + SAMPLES {
        let started = Instant::now();
        handshake(&mut client, &server.addr);
        let elapsed = started.elapsed();
        if i >= WARMUP {
            samples.push(elapsed);
        }
    }
    drop(server);
    report(label, &samples);
}

fn probe_echo(label: &str, limits: QuicLimits) {
    let server = Server::spawn(limits.clone(), Mode::Echo);
    let mut samples = Vec::with_capacity(SAMPLES);
    let mut client = bind_client(limits);
    for i in 0..WARMUP + SAMPLES {
        let started = Instant::now();
        echo(&mut client, &server.addr);
        let elapsed = started.elapsed();
        if i >= WARMUP {
            samples.push(elapsed);
        }
    }
    drop(server);
    report(label, &samples);
}

fn probe_rss() {
    let empty = rss_kb();
    println!("rss empty process after previous probes: {empty} kB (noisy; see isolated run below)");

    let counts = [1usize, 50, 200];
    for &count in &counts {
        let (baseline, held, client_ids, server_ids) = rss_idle_pair(count);
        let delta = held.saturating_sub(baseline);
        let per = delta / count as u64;
        println!(
            "  idle n={count:<3}  baseline={baseline} kB  held={held} kB  delta={delta} kB  ~{per} kB/conn  (client={client_ids} server={server_ids})"
        );
    }
}

fn rss_idle_pair(count: usize) -> (u64, u64, usize, usize) {
    let limits = QuicLimits {
        idle_timeout_ms: 3_600_000,
        require_address_validation: false,
        max_connections: count + 8,
        ..QuicLimits::default()
    };
    let mut server = QuicTransport::new(
        QuicNodeConfig::generate().with_limits(limits.clone()),
        "127.0.0.1:0",
    )
    .expect("server");
    let mut client = QuicTransport::new(
        QuicNodeConfig::generate().with_limits(limits),
        "127.0.0.1:0",
    )
    .expect("client");
    server.listen_on_bound_addr().expect("listen");
    let addr = server.local_peer_addr().expect("addr");
    let server_peer = addr.peer_id().clone();
    let client_peer = client.local_peer_id();
    let baseline = rss_kb();
    let mut clock = StdClock::with_epoch(Instant::now());
    let mut started = 0;
    while started != count {
        client.dial(&addr).expect("dial");
        started += 1;
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.connection_ids_for_peer(&client_peer).len() != started
            || client.connection_ids_for_peer(&server_peer).len() != started
        {
            drop(server.poll(clock.now()).expect("server poll"));
            drop(client.poll(clock.now()).expect("client poll"));
            assert!(Instant::now() < deadline, "idle pair setup timed out");
            if server.connection_ids_for_peer(&client_peer).len() != started
                || client.connection_ids_for_peer(&server_peer).len() != started
            {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    drop(server.poll(clock.now()).expect("server poll"));
    drop(client.poll(clock.now()).expect("client poll"));
    let held = rss_kb();
    (
        baseline,
        held,
        client.connection_ids_for_peer(&server_peer).len(),
        server.connection_ids_for_peer(&client_peer).len(),
    )
}

fn handshake(client: &mut QuicTransport, addr: &PeerAddr) {
    let id = client.dial(addr).expect("dial");
    drive_client(
        client,
        |event| matches!(event, TransportEvent::Connected { id: got, .. } if *got == id),
    );
    match client.close(id) {
        Ok(()) | Err(_) => {}
    }
    let mut clock = StdClock::with_epoch(Instant::now());
    drop(client.poll(clock.now()).expect("reap poll"));
}

fn echo(client: &mut QuicTransport, addr: &PeerAddr) {
    let id = client.dial(addr).expect("dial");
    drive_client(
        client,
        |event| matches!(event, TransportEvent::Connected { id: got, .. } if *got == id),
    );
    let stream = client.open_stream(id).expect("open");
    client.send_stream(id, stream, ECHO.to_vec()).expect("send");
    drive_client(client, |event| {
        matches!(
            event,
            TransportEvent::StreamData {
                id: got,
                data,
                ..
            } if *got == id && data == ECHO
        )
    });
    match client.close(id) {
        Ok(()) | Err(_) => {}
    }
    let mut clock = StdClock::with_epoch(Instant::now());
    drop(client.poll(clock.now()).expect("reap poll"));
}

fn drive_client(client: &mut QuicTransport, mut done: impl FnMut(&TransportEvent) -> bool) {
    let mut clock = StdClock::with_epoch(Instant::now());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "client drive timed out");
        let events = client.poll(clock.now()).expect("client poll");
        if events.iter().any(&mut done) {
            return;
        }
        client.wait_for_input(WAIT);
    }
}

fn bind_client(limits: QuicLimits) -> QuicTransport {
    QuicTransport::new(
        QuicNodeConfig::generate().with_limits(limits),
        "127.0.0.1:0",
    )
    .expect("client bind")
}

#[derive(Clone, Copy)]
enum Mode {
    Handshake,
    Echo,
}

struct Server {
    addr: PeerAddr,
    stop: Arc<AtomicBool>,
    interrupt: WaitHandle,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn spawn(limits: QuicLimits, mode: Mode) -> Self {
        let mut server = QuicTransport::new(
            QuicNodeConfig::generate().with_limits(limits),
            "127.0.0.1:0",
        )
        .expect("server bind");
        server.listen_on_bound_addr().expect("listen");
        let addr = server.local_peer_addr().expect("addr");
        let interrupt = server.wait_handle();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut clock = StdClock::with_epoch(Instant::now());
            let mut echo_streams = Vec::new();
            while !worker_stop.load(Ordering::Relaxed) {
                let events = server.poll(clock.now()).expect("server poll");
                for event in events {
                    match (mode, event) {
                        (_, TransportEvent::Connected { id, .. }) => {
                            if matches!(mode, Mode::Handshake) {
                                match server.close(id) {
                                    Ok(()) | Err(_) => {}
                                }
                            }
                        }
                        (
                            Mode::Echo,
                            TransportEvent::StreamData {
                                id,
                                stream_id,
                                data,
                            },
                        ) => {
                            echo_streams.push((id, stream_id));
                            match server.send_stream(id, stream_id, data) {
                                Ok(()) | Err(_) => {}
                            }
                        }
                        (_, TransportEvent::Closed { id }) => {
                            echo_streams.retain(|(conn, _)| *conn != id);
                        }
                        _ => {}
                    }
                }
                server.wait_for_input(WAIT);
            }
        });
        Self {
            addr,
            stop,
            interrupt,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.interrupt.interrupt();
        if let Some(thread) = self.thread.take() {
            thread.join().expect("server thread");
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
            let kb = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            return kb;
        }
    }
    0
}
