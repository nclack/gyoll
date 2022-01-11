use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{sleep, spawn, JoinHandle, self},
    time::Duration, panic, process,
};

use gyoll::base::{Channel, ChannelFactory, Receiver, Sender};

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
                println!("tick");
                sleep(Duration::from_millis(200));
            }
        }),
    }
}

fn producer(tx: Sender, name: &'static str) -> (JoinHandle<()>, &str) {
    let mut tx = tx;
    let th = thread::Builder::new().name(name.into()).spawn(move || {
        println!("{}: Entering Writer", name);
        let mut written_bytes = 0;
        while let Some(mut buf) = tx.map(13) {
            for (k, v) in buf.iter_mut().zip(0u16..) {
                *k = v as u8;
            }
            written_bytes += buf.len();
            // sleep(Duration::from_millis(2));
        }
        println!("{}: Write - total: {}", name, written_bytes);
        println!("{}: Exiting Writer", name);
    }).unwrap();
    (th, name)
}

fn consumer(rx: Receiver, name: &'static str) -> (JoinHandle<()>, &str) {
    let mut rx = rx;
    let th = thread::Builder::new().name(name.into()).spawn(move || {
        println!("{}: Entering Reader", name);
        let mut read_bytes = 0;
        let mut first = None;
        while rx.is_open() {
            while let Some(available) = rx.next() {
                if first.is_none() {
                    first = Some(available.as_ptr() as isize);
                }
                read_bytes += available.len();
                println!(
                    "{}: 0x{:0x} {:4}:{:4}",
                    // "{}: 0x{:0x} {:4}:{:4} - {:?}",
                    name,
                    available.as_ptr() as isize,
                    available.as_ptr() as isize - first.unwrap(),
                    available.as_ptr() as isize - first.unwrap() + available.len() as isize,
                    // &available[0..std::cmp::min(20, available.len())]
                );
                sleep(Duration::from_millis(1));
            }
        }
        println!(
            "{}: Read - total: {} ({} GB)",
            name,
            read_bytes,
            (read_bytes as f32) * 1e-9
        );
        println!("{}: Exiting Reader", name);
    }).unwrap();
    (th, name)
}

fn main() {

    // Install a custom panic handler so the process exits if there's a 
    // panic in a thread.

    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    // Get down to business

    let ch = Arc::new(Channel::new(1 << 12));

    let threads = [
        consumer(ch.receiver(), "R0"),
        consumer(ch.receiver(), "R1"),
        // consumer(ch.receiver(), "R2"),
        producer(ch.sender(),   "W0"),
    ];

    let ticker = ticker();
    sleep(Duration::from_secs(1));
    ch.close();
    println!("Stop");
    ticker.running.store(false, Ordering::SeqCst);

    for (t, name) in threads {
        t.join().expect(&format!("{} failed", name));
    }
}
