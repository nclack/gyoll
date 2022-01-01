use std::{
    sync::Arc,
    thread::{spawn, JoinHandle},
};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gyoll::base::{Channel, ChannelFactory};
use parking_lot::{Condvar, Mutex};

fn criterion_benchmark(c: &mut Criterion) {
    let env = Env::new(1 << 24);
    c.bench_function("spsc 1M 1k", |b| {
        b.iter_batched(
            || env.ready(1 << 12),
            |ready| ready.run(black_box(0)),
            criterion::BatchSize::PerIteration,
        );
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

macro_rules! debug {
    ($($e:expr),*) => {println!( $($e),* )}
}

struct Env {
    channel: Arc<Channel>,
}

struct ReadyEnv {
    _producer: JoinHandle<()>,
    _consumer: JoinHandle<()>,
    is_started: Arc<Mutex<bool>>,
    is_done: Arc<Mutex<bool>>,
    start: Arc<Condvar>,
    done: Arc<Condvar>,
}

impl Env {
    fn new(capacity: usize) -> Self {
        Env {
            channel: Arc::new(Channel::new(capacity)),
        }
    }

    fn ready(&self, chunk: usize) -> ReadyEnv {
        let (mut tx, mut rx) = (self.channel.sender(), self.channel.receiver());
        let is_started = Arc::new(Mutex::new(false));
        let is_done = Arc::new(Mutex::new(false));
        let start = Arc::new(Condvar::new());
        let done = Arc::new(Condvar::new());
        let payload_size = 1 << 30;

        let producer = {
            let is_started = is_started.clone();
            let start = start.clone();
            spawn(move || {
                let mut is_started = is_started.lock();
                loop {
                    debug!("Prod - Awaiting start");
                    if !*is_started {
                        start.wait(&mut is_started);
                        debug!("Prod - Start triggered");
                    }
                    debug!("Prod - Start");

                    let mut remaining_bytes = payload_size;
                    while remaining_bytes >= chunk {
                        let buf = tx.map(chunk).unwrap();
                        remaining_bytes -= buf.len();
                    }

                    *is_started = false;
                    debug!("Prod - Done");
                }
            })
        };

        let consumer = {
            let is_done = is_done.clone();
            let done = done.clone();
            spawn(move || loop {
                debug!("Cons - Start");

                let mut received_bytes = 0;
                while rx.is_open() && received_bytes < payload_size {
                    while let Some(buf) = rx.next() {
                        received_bytes += buf.len();
                    }
                }
                {
                    let mut is_done = is_done.lock();
                    *is_done = true;
                    debug!("Cons - Done: {} bytes", received_bytes);
                    done.notify_all();
                }
            })
        };

        ReadyEnv {
            _producer: producer,
            _consumer: consumer,
            is_started,
            is_done,
            start,
            done,
        }
    }
}

impl ReadyEnv {
    fn run(&self, iter: u64) {
        let mut is_done = self.is_done.lock();
        *is_done = false;

        debug!(" Run - Start ***** {}", iter);
        {
            let mut is_started = self.is_started.lock();
            *is_started = true;
            self.start.notify_all();
        }

        debug!(" Run - Awaiting done - {}", *is_done);
        if !*is_done {
            self.done.wait(&mut is_done);
            debug!(" Run - Done triggered");
        }
        debug!(" Run - Done\n");
    }
}
