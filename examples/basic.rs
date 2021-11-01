#![allow(unused_variables)]

use std::{
    thread::{sleep, spawn},
    time::Duration,
};

use anyhow::{anyhow, Result};
use gyoll::base::Channel;

fn main() -> Result<()> {
    let c = Channel::new(1 << 10);

    let mut tx = c.sender();
    let mut rx = c.receiver();

    let producer = spawn(move || {
        // NOTE: name - map() - alternatives get,request,

        while let Some(mut buf) = tx.map(13) {
            // NOTE: tx get's mutable borrowed by map() so that
            //       calling it here becomes a compiler error
            // tx.map(13); // <-- doesn't work
            println!("Write");

            for (k, v) in buf.iter_mut().zip(0u8..) {
                *k = v;
            }
            sleep(Duration::from_millis(200));
        }
    });

    let consumer = spawn(move || {
        // TODO: what are the shutdown/disconnect rules?
        while rx.is_open() {
            // TODO: get rid of is_open()...use some other iterable or change next
            while let Some(available) = rx.next() {
                // rx.next(); // <-- doesn't work bc rx is mut
                println!("Read");
                println!("{:?}", available.as_ref());
                sleep(Duration::from_millis(1000));
            }
        }
    });

    sleep(Duration::from_secs(5));
    c.stop();
    println!("STOP");

    producer.join().map_err(|_| anyhow!("Producer failed"))?;
    consumer.join().map_err(|_| anyhow!("Consumer failed"))?;

    Ok(())
}
