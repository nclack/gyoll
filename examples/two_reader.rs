use std::{
    panic, process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, sleep, spawn, JoinHandle},
    time::Duration,
};

use gyoll::base::{Channel, ChannelFactory, Receiver, Sender};
use log::{debug, info};

struct Ticker {
    _thread: JoinHandle<()>,
    running: Arc<AtomicBool>,
}

fn ticker() -> Ticker {
    let running = Arc::new(AtomicBool::new(true));
    Ticker {
        running: running.clone(),
        _thread: spawn(move || {
            while running.load(Ordering::Relaxed) {
                debug!("tick");
                sleep(Duration::from_millis(200));
            }
        }),
    }
}

fn producer(tx: Sender, name: &'static str, payload_size: usize) -> (JoinHandle<()>, &str) {
    let mut tx = tx;
    let th = thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            info!("{}: Entering Writer", name);
            let mut written_bytes = 0;
            let first = tx.channel().as_ptr() as isize;
            while let Some(buf) = tx.map(payload_size) {
                written_bytes += buf.len();
                debug!(
                    "{}: 0x{:0x} {:4}:{:4}",
                    name,
                    buf.as_ptr() as isize,
                    buf.as_ptr() as isize - first,
                    buf.as_ptr() as isize - first + buf.len() as isize
                );
                // sleep(Duration::from_millis(10));
            }
            info!(
                "{}: Write - total: {} bytes. {} ops",
                name,
                written_bytes,
                written_bytes / payload_size
            );
            info!("{}: Exiting Writer", name);
        })
        .unwrap();
    (th, name)
}

fn consumer(rx: Receiver, name: &'static str) -> (JoinHandle<()>, &str) {
    let mut rx = rx;
    let th = thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            info!("{}: Entering Reader", name);
            let mut read_bytes = 0;
            let mut first = None;
            while rx.is_open() {
                while let Some(available) = rx.next() {
                    if first.is_none() {
                        first = Some(available.as_ptr() as isize);
                    }
                    read_bytes += available.len();
                    debug!(
                        "{}: 0x{:0x} {:4}:{:4}",
                        // "{}: 0x{:0x} {:4}:{:4} - {:?}",
                        name,
                        available.as_ptr() as isize,
                        available.as_ptr() as isize - first.unwrap(),
                        available.as_ptr() as isize - first.unwrap() + available.len() as isize,
                        // &available[0..std::cmp::min(20, available.len())]
                    );
                    // sleep(Duration::from_millis(10));
                }
            }
            info!(
                "{}: Read - total: {} ({} GB)",
                name,
                read_bytes,
                (read_bytes as f32) * 1e-9
            );
            info!("{}: Exiting Reader", name);
        })
        .unwrap();
    (th, name)
}

fn main() {
    pretty_env_logger::init();
    // Install a custom panic handler so the process exits if there's a
    // panic in a thread.

    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    // Get down to business

    let ch = Arc::new(Channel::new(1 << 16));

    let threads = [
        consumer(ch.receiver(), "R0"),
        consumer(ch.receiver(), "R1"),
        consumer(ch.receiver(), "R2"),
        producer(ch.sender(), "W0", 1 << 12),
        producer(ch.sender(), "W1", 17),
    ];

    let ticker = ticker();
    sleep(Duration::from_secs(1));
    ch.close();
    info!("Stop");
    ticker.running.store(false, Ordering::SeqCst);

    for (t, name) in threads {
        t.join().expect(&format!("{} failed", name));
    }
}
