#![allow(unused_variables)]

use std::{
    thread::{sleep, spawn},
    time::Duration,
};

use anyhow::{anyhow, Result};
use gyoll::Channel;

fn main() -> Result<()> {
    let c = Channel::new(1 << 10);

    let (mut tx, rx) = { (c.sender(), c.receiver()) };

    let producer = spawn(move || {
        // NOTE: name - map() - alternatives get,request,

        while let Some(mut buf) = tx.map(13) {
            // NOTE: tx get's mutable borrowed by map() so that
            //       calling it here becomes a compiler error
            // tx.map(13); // <-- doesn't work

            for (k, v) in buf.iter_mut().zip(0u8..) {
                *k = v;
            }
        }
    });

    let consumer = spawn(move || {
        while let Some(available) = rx.acquire() {
            for item in available {
                todo!()
            }
        }
    });

    sleep(Duration::from_secs(5));
    // TODO: signal stop
    producer.join().map_err(|_| anyhow!("Producer failed"))?;
    consumer.join().map_err(|_| anyhow!("Consumer failed"))?;

    Ok(())
}
