//! Work done off the window's thread, so that the window never waits: looking at the game's
//! folder, waiting for a game to end, asking the system something. Each piece of work runs on a
//! thread of its own and leaves what it came to in a queue; the window is woken to take it (it
//! sleeps the rest of the time: nothing is polled).
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

/// What wakes whoever takes the results (the window's event loop; nothing, in a test).
pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Jobs<M> {
    tx: Sender<M>,
    rx: Receiver<M>,
    wake: Wake,
}

impl<M: Send + 'static> Jobs<M> {
    pub fn new(wake: Wake) -> Jobs<M> {
        let (tx, rx) = channel();
        Jobs { tx, rx, wake }
    }

    /// Do `work` on a thread of its own; what it comes to is left to be taken.
    pub fn spawn(&self, work: impl FnOnce() -> M + Send + 'static) {
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            // (nobody left to take it: the launcher is closing)
            if tx.send(work()).is_ok() {
                wake();
            }
        });
    }

    /// The next result that is ready, if any.
    pub fn take(&self) -> Option<M> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    #[test]
    fn work_is_done_elsewhere_and_its_result_wakes_whoever_takes_it() {
        let woken = Arc::new(AtomicUsize::new(0));
        let count = woken.clone();
        let jobs: Jobs<u32> = Jobs::new(Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(jobs.take(), None);
        let here = std::thread::current().id();
        jobs.spawn(move || u32::from(std::thread::current().id() != here));
        jobs.spawn(|| 7);
        let limit = Instant::now() + Duration::from_secs(10);
        let mut taken = Vec::new();
        while taken.len() < 2 {
            assert!(Instant::now() < limit, "the work never ended");
            match jobs.take() {
                Some(result) => taken.push(result),
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        taken.sort_unstable();
        assert_eq!(taken, [1, 7]);
        // (each result woke the taker, once it was there to be taken)
        while woken.load(Ordering::SeqCst) < 2 {
            assert!(Instant::now() < limit, "the taker was not woken");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(jobs.take(), None);
    }
}
