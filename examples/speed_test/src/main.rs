//! Speed test example for Matchbox WebRTC sockets.
//!
//! Run with: cargo run --manifest-path examples/speed_test/Cargo.toml
//! Automatically starts a signaling server and runs sender + receiver clients.

use futures::{FutureExt, select};
use futures_timer::Delay;
use matchbox_socket::{PeerState, WebRtcSocket};
use std::time::{Duration, Instant};

const CHANNEL_ID: usize = 0;
/// Size of each test packet in bytes
const PACKET_SIZE: usize = 60_000;
/// Number of packets to send
const PACKET_COUNT: usize = 1700; // ~102MB total
const TOTAL_SIZE: usize = PACKET_SIZE * PACKET_COUNT;

const SERVER_URL: &str = "ws://127.0.0.1:3536/speed_test";
/// Special message sent by receiver to signal completion
const DONE_MSG: &[u8] = b"DONE";

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap();
    wasm_bindgen_futures::spawn_local(async {
        log::error!("WASM mode: not supported");
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    use matchbox_signaling::SignalingServerBuilder;
    use matchbox_signaling::topologies::full_mesh::{FullMesh, FullMeshState};
    use tracing_subscriber::prelude::*;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "speed_test_example=info,matchbox_socket=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 1. Start the signaling server
    log::info!("Starting signaling server on 127.0.0.1:3536...");
    let server_addr: std::net::SocketAddr = "127.0.0.1:3536".parse().unwrap();
    let server =
        SignalingServerBuilder::new(server_addr, FullMesh, FullMeshState::default()).build();
    tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            log::error!("Signaling server error: {e}");
        }
    });

    // 2. Wait for server to be ready
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 3. Spawn sender in background
    log::info!("Starting sender (background)...");
    let sender_handle = tokio::spawn(async move {
        run_sender().await
    });

    // 4. Wait before starting receiver
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 5. Run receiver (blocks until test completes)
    log::info!("Starting receiver...");
    let (receiver_mbps, receiver_bytes, receiver_elapsed) = run_receiver().await;

    // 6. Wait for sender to finish
    let sender_mbps = sender_handle.await.unwrap();

    log::info!("=== Speed Test Results ===");
    log::info!("Sender enqueue rate: {:.2} Mbps", sender_mbps);
    log::info!(
        "Receiver throughput: {:.2} Mbps ({} bytes in {:.3}s)",
        receiver_mbps,
        receiver_bytes,
        receiver_elapsed
    );
}

/// Runs the sender: connects, sends data, waits for completion signal.
/// Returns the sender's enqueue rate in Mbps.
async fn run_sender() -> f64 {
    let (mut socket, loop_fut) = WebRtcSocket::new_reliable(SERVER_URL);

    let loop_fut = loop_fut.fuse();
    futures::pin_mut!(loop_fut);

    let timeout = Delay::new(Duration::from_millis(100));
    futures::pin_mut!(timeout);

    let mut peer_id = None;

    // Phase 1: wait for peer to connect
    loop {
        for (peer, state) in socket.update_peers() {
            match state {
                PeerState::Connected => {
                    log::info!("[sender] Peer connected: {peer}");
                    peer_id = Some(peer);
                }
                PeerState::Disconnected => {
                    log::info!("[sender] Peer left: {peer}");
                    return 0.0;
                }
            }
        }
        for (_peer, _packet) in socket.channel_mut(CHANNEL_ID).receive() {}

        select! {
            _ = (&mut timeout).fuse() => {
                timeout.reset(Duration::from_millis(100));
            }
            _ = &mut loop_fut => {
                log::info!("[sender] Socket loop ended");
                return 0.0;
            }
        }

        if peer_id.is_some() {
            break;
        }
    }

    let peer = peer_id.unwrap();

    // Phase 2: send data
    let payload: Box<[u8]> = (0..PACKET_SIZE)
        .map(|i| (i % 256) as u8)
        .collect::<Vec<_>>()
        .into_boxed_slice();

    let start = Instant::now();

    for i in 0..PACKET_COUNT {
        socket.channel_mut(CHANNEL_ID).send(payload.clone(), peer);
        tokio::task::yield_now().await;
        if i % 200 == 0 {
            log::info!("[sender] Sent {}/{}", i, PACKET_COUNT);
        }
    }

    let elapsed = start.elapsed();
    let mbps = (TOTAL_SIZE as f64 * 8.0) / elapsed.as_secs_f64() / 1_000_000.0;
    log::info!(
        "[sender] Enqueued {} bytes in {:.3?} ({:.2} Mbps)",
        TOTAL_SIZE,
        elapsed,
        mbps
    );

    // Phase 3: wait for receiver to signal completion
    log::info!("[sender] Waiting for receiver completion signal...");
    loop {
        for (_peer, state) in socket.update_peers() {
            match state {
                PeerState::Disconnected => {
                    log::info!("[sender] Peer disconnected");
                    return 0.0;
                }
                PeerState::Connected => {}
            }
        }
        for (_p, packet) in socket.channel_mut(CHANNEL_ID).receive() {
            if &packet[..] == DONE_MSG {
                log::info!("[sender] Receiver confirmed completion");
                return mbps;
            }
        }

        select! {
            _ = (&mut timeout).fuse() => {
                timeout.reset(Duration::from_millis(100));
            }
            _ = &mut loop_fut => {
                log::info!("[sender] Socket loop ended");
                return 0.0;
            }
        }
    }
}

/// Runs the receiver: connects, receives data, signals completion.
/// Returns (throughput in Mbps, bytes received, elapsed seconds).
async fn run_receiver() -> (f64, usize, f64) {
    let (mut socket, loop_fut) = WebRtcSocket::new_reliable(SERVER_URL);

    let loop_fut = loop_fut.fuse();
    futures::pin_mut!(loop_fut);

    let timeout = Delay::new(Duration::from_millis(100));
    futures::pin_mut!(timeout);

    // Phase 1: wait for peer to connect
    loop {
        for (_peer, state) in socket.update_peers() {
            match state {
                PeerState::Connected => {
                    log::info!("[receiver] Peer connected");
                }
                PeerState::Disconnected => {
                    log::info!("[receiver] Peer disconnected");
                    return (0.0, 0, 0.0);
                }
            }
        }
        for _ in socket.channel_mut(CHANNEL_ID).receive() {}

        select! {
            _ = (&mut timeout).fuse() => {
                timeout.reset(Duration::from_millis(100));
            }
            _ = &mut loop_fut => {
                log::info!("[receiver] Socket loop ended");
                return (0.0, 0, 0.0);
            }
        }

        if socket.connected_peers().count() > 0 {
            break;
        }
    }

    // Phase 2: receive data
    log::info!("[receiver] Waiting for data...");
    let mut bytes_received = 0usize;
    let mut start: Option<Instant> = None;
    let mut mbps = 0.0;

    loop {
        for (_peer, state) in socket.update_peers() {
            match state {
                PeerState::Disconnected => {
                    log::info!("[receiver] Peer disconnected");
                    let elapsed_secs = start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
                    mbps = if elapsed_secs > 0.0 {
                        (bytes_received as f64 * 8.0) / elapsed_secs / 1_000_000.0
                    } else {
                        0.0
                    };
                    return (mbps, bytes_received, elapsed_secs);
                }
                PeerState::Connected => {}
            }
        }

        for (_peer, packet) in socket.channel_mut(CHANNEL_ID).receive() {
            if start.is_none() {
                start = Some(Instant::now());
            }
            bytes_received += packet.len();
            log::info!("[receiver] Progress: {} bytes", bytes_received);
        }

        select! {
            _ = (&mut timeout).fuse() => {
                timeout.reset(Duration::from_millis(100));
            }
            _ = &mut loop_fut => {
                log::info!("[receiver] Socket loop ended");
                break;
            }
        }
    }

    let elapsed_secs = start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
    let mbps = if elapsed_secs > 0.0 {
        (bytes_received as f64 * 8.0) / elapsed_secs / 1_000_000.0
    } else {
        0.0
    };
    log::info!(
        "[receiver] Received {} bytes in {:.3}s = {:.2} Mbps",
        bytes_received,
        elapsed_secs,
        mbps
    );

    // Phase 3: send completion signal to sender
    log::info!("[receiver] Signaling completion to sender...");
    let peers: Vec<_> = socket.connected_peers().collect();
    for peer in peers {
        socket
            .channel_mut(CHANNEL_ID)
            .send(DONE_MSG.to_vec().into_boxed_slice(), peer);
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    log::info!("[receiver] Done");

    (mbps, bytes_received, elapsed_secs)
}
