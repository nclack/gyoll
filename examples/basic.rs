#![allow(unused_variables)]

use std::{
    panic, process,
    sync::Arc,
    thread::{sleep, spawn},
    time::Duration,
};

use gyoll::base::channel;
use log::{debug, info};
use parking_lot::Mutex;

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

    let (mut tx, mut rx) = channel(1 << 12);
    let ch = tx.channel().clone();

    let ticker_running = Arc::new(Mutex::new(true));
    {
        let running = ticker_running.clone();
        let ticker = spawn(move || {
            while *running.lock() {
                info!("tick");
                sleep(Duration::from_millis(200));
            }
        });
    }

    let producer = spawn(move || {
        // NOTE: name - map() - alternatives get,request,
        info!("Entering Writer");
        let mut written_bytes = 0;
        while let Some(mut buf) = tx.map(17) {
            // NOTE: tx get's mutable borrowed by map() so that
            //       calling it here becomes a compiler error
            // tx.map(13); // <-- doesn't work
            for (k, v) in buf.iter_mut().zip(0u16..) {
                *k = v as u8;
            }
            written_bytes += buf.len();
            // sleep(Duration::from_millis(1));
        }
        info!("Write - total: {}", written_bytes);
        info!("Exiting Writer");
    });

    let consumer = spawn(move || {
        info!("Entering Reader");
        let mut read_bytes = 0;
        // TODO: what are the shutdown/disconnect rules?
        let first = rx.channel().as_ptr() as isize;
        while rx.is_open() {
            // TODO: get rid of is_open()...use some other iterable or change next
            while let Some(available) = rx.next() {
                // rx.next(); // <-- doesn't work bc rx is mut
                read_bytes += available.len();
                debug!(
                    "0x{:0x} {:4}:{:4} {}",
                    available.as_ptr() as isize,
                    available.as_ptr() as isize - first,
                    available.as_ptr() as isize - first + available.len() as isize,
                    available.cycle()
                );
                // sleep(Duration::from_millis(10));
            }
        }
        info!(
            "Read - total: {} ({} GB)",
            read_bytes,
            (read_bytes as f32) * 1e-9
        );
        info!("Exiting Reader");
    });

    sleep(Duration::from_secs(1));
    ch.close();
    info!("STOP");
    *ticker_running.lock() = false;

    producer.join().expect("Producer failed");
    consumer.join().expect("Consumer failed");
}
