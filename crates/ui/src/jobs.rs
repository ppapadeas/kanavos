//! Background computation of derived data keyed by document revision (docs/architecture.md §3).
//!
//! The interface never waits: a [`Background`] starts the computation on a worker thread when the revision
//! changes, keeps showing the last finished result meanwhile, and swaps in the new one when it arrives. Results
//! for revisions that are no longer wanted are dropped.
//!
//! **Adapter note.** When `kanavos_engine::derived` lands, its scheduler can replace the thread-per-request
//! strategy here without changing callers.

use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// A value computed in the background for a document revision.
pub struct Background<T: Send + 'static> {
    name: &'static str,
    latest: Option<(u64, T)>,
    running: Option<(u64, Receiver<T>)>,
}

impl<T: Send + 'static> Background<T> {
    /// An empty slot; `name` labels the worker thread.
    pub fn new(name: &'static str) -> Self {
        Background { name, latest: None, running: None }
    }

    /// Collects a finished result, if any.
    pub fn poll(&mut self) {
        if let Some((rev, rx)) = &self.running {
            match rx.try_recv() {
                Ok(v) => {
                    self.latest = Some((*rev, v));
                    self.running = None;
                }
                Err(TryRecvError::Disconnected) => self.running = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    /// Makes sure a result for `revision` is (being) computed. `job` runs only when a computation starts;
    /// `wake` is called from the worker when the result is ready (to request a repaint).
    pub fn request(
        &mut self,
        revision: u64,
        job: impl FnOnce() -> T + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) {
        self.poll();
        if self.latest.as_ref().is_some_and(|(r, _)| *r == revision)
            || self.running.as_ref().is_some_and(|(r, _)| *r == revision)
        {
            return;
        }
        let (tx, rx) = channel();
        let started = std::thread::Builder::new().name(format!("kanavos-{}", self.name)).spawn(move || {
            let v = job();
            if tx.send(v).is_ok() {
                wake();
            }
        });
        match started {
            Ok(_) => self.running = Some((revision, rx)),
            Err(_) => self.running = None,
        }
    }

    /// The last finished result and the revision it belongs to.
    pub fn latest(&self) -> Option<(u64, &T)> {
        self.latest.as_ref().map(|(r, v)| (*r, v))
    }

    /// Whether a computation is in progress.
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Forgets results (e.g. after the language changed and texts must be recomputed).
    pub fn invalidate(&mut self) {
        self.latest = None;
        self.running = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait<T: Send + 'static>(b: &mut Background<T>) {
        let start = Instant::now();
        while b.busy() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(2));
            b.poll();
        }
    }

    #[test]
    fn results_arrive_for_the_requested_revision() {
        let mut b = Background::new("test");
        b.request(1, || 41 + 1, || ());
        wait(&mut b);
        assert_eq!(b.latest(), Some((1, &42)));
    }

    #[test]
    fn the_last_result_stays_until_the_new_one_arrives() {
        let mut b = Background::new("test");
        b.request(1, || "one", || ());
        wait(&mut b);
        b.request(2, || "two", || ());
        // Until the worker finishes, the previous result is still available.
        assert!(b.latest().is_some());
        wait(&mut b);
        assert_eq!(b.latest(), Some((2, &"two")));
    }

    #[test]
    fn repeated_requests_for_the_same_revision_do_not_restart() {
        let mut b = Background::new("test");
        b.request(3, || 1, || ());
        wait(&mut b);
        b.request(3, || 2, || ());
        assert!(!b.busy());
        assert_eq!(b.latest(), Some((3, &1)));
    }
}
