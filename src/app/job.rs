//! Work done on another thread, picked up by the app once it's ready.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use super::Waker;

/// Slow work (running a command, reading a big file, asking a server) done on another
/// thread, so the app keeps drawing meanwhile. When the work is done the app is sent an
/// [`Event::Wake`](super::Event::Wake), and [`poll`](Job::poll) hands the result over.
///
/// To start the work again, make a new job in the old one's place: whatever the old one
/// finds is dropped with it, so a stale answer never turns up after a newer one.
///
/// ```no_run
/// use tinsel::app::{self, App, Event, Flow, Frame, Job};
///
/// struct Status {
///     job: Option<Job<String>>,
///     shown: String,
/// }
///
/// impl App for Status {
///     fn view(&self, frame: &mut Frame) {
///         frame.print(frame.area(), 0, &self.shown);
///     }
///
///     fn event(&mut self, event: &Event) -> Flow {
///         if let Some(done) = self.job.as_mut().and_then(Job::poll) {
///             self.shown = done;
///             self.job = None;
///         }
///         match event {
///             Event::Key(app::Key::Char('q')) => Flow::Quit,
///             _ => Flow::Continue,
///         }
///     }
/// }
///
/// let job = Job::spawn(|| "all healthy".to_string());
/// app::run(Status { job: Some(job), shown: "checking…".into() })?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Job<T> {
    out: Arc<Mutex<Option<T>>>,
    thread: Option<JoinHandle<()>>,
}

impl<T> std::fmt::Debug for Job<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("finished", &self.is_finished())
            .finish_non_exhaustive()
    }
}

impl<T: Send + 'static> Job<T> {
    /// Start `work` on a thread of its own.
    pub fn spawn(work: impl FnOnce() -> T + Send + 'static) -> Self {
        let out = Arc::new(Mutex::new(None));
        let into = Arc::downgrade(&out);
        let waker = Waker::new();
        let thread = std::thread::spawn(move || {
            // woken however the work ends, panicking included, so the app looks
            struct Wake(Waker);
            impl Drop for Wake {
                fn drop(&mut self) {
                    self.0.wake();
                }
            }
            let _wake = Wake(waker);
            let done = work();
            // nobody to hand it to once the job's been dropped
            if let Some(into) = into.upgrade() {
                *into.lock().unwrap_or_else(PoisonError::into_inner) = Some(done);
            }
        });
        Job {
            out,
            thread: Some(thread),
        }
    }
}

impl<T> Job<T> {
    /// The result, once the work is done: `Some` the first time it's asked for after that,
    /// and `None` before and after.
    pub fn poll(&mut self) -> Option<T> {
        self.out.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    /// Whether the work has ended, with a result or by panicking.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Wait for the work to end and take its result, if it gave one and it hasn't been
    /// taken: for tests, and for an app that can't go on without it.
    pub fn wait(mut self) -> Option<T> {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.poll()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Jobs wake whichever app is listening, so they keep out of the way of the tests of
    /// waking.
    fn wakes() -> std::sync::MutexGuard<'static, ()> {
        crate::app::tests::WAKES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn the_result_is_handed_over_once() {
        let _wakes = wakes();
        let mut job = Job::spawn(|| 6 * 7);
        while !job.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(job.poll(), Some(42));
        assert_eq!(job.poll(), None);
        assert!(format!("{job:?}").contains("finished: true"));
    }

    #[test]
    fn waiting_gives_the_result_or_nothing_after_a_panic() {
        let _wakes = wakes();
        assert_eq!(Job::spawn(|| "done").wait(), Some("done"));
        assert_eq!(Job::spawn(|| -> u8 { panic!("broke on purpose") }).wait(), None);
    }

    #[test]
    fn nothing_turns_up_before_the_work_is_done() {
        let _wakes = wakes();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut job = Job::spawn(move || rx.recv().is_ok());
        assert_eq!(job.poll(), None);
        assert!(!job.is_finished());
        tx.send(()).unwrap();
        assert_eq!(job.wait(), Some(true));
    }
}
