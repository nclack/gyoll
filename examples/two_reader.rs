use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{sleep, spawn, JoinHandle},
    time::Duration,
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
    let th = spawn(move || {
        // NOTE: name - map() - alternatives get,request,
        println!("{}: Entering Writer", name);
        let mut written_bytes = 0;
        // FIXME: There's some problem when the chunksize is >= (N/2)+1 - won't proceed
        while let Some(mut buf) = tx.map(13) {
            // NOTE: tx get's mutable borrowed by map() so that
            //       calling it here becomes a compiler error
            // tx.map(13); // <-- doesn't work
            for (k, v) in buf.iter_mut().zip(0u16..) {
                *k = v as u8;
            }
            written_bytes += buf.len();
            // println!("{}: Write - total: {}", name, written_bytes);
            // sleep(Duration::from_millis(2));
        }
        println!("{}: Write - total: {}", name, written_bytes);
        println!("{}: Exiting Writer", name);
    });
    (th, name)
}

fn consumer(rx: Receiver, name: &'static str) -> (JoinHandle<()>, &str) {
    let mut rx = rx;
    let th = spawn(move || {
        println!("{}: Entering Reader", name);
        let mut read_bytes = 0;
        // TODO: what are the shutdown/disconnect rules?
        let mut first = None;
        while rx.is_open() {
            // TODO: get rid of is_open()...use some other iterable or change next
            while let Some(available) = rx.next() {
                // rx.next(); // <-- doesn't work bc rx is mut
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
    });
    (th, name)
}

fn main() {
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
