//! Minimal single-future executor that parks the calling thread while pending.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Drives `future` to completion on the current thread, parking between polls.
///
/// A wake that arrives before `park` is not lost: `unpark` leaves a token.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::block_on;
    use std::future::{self, Future};
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Waker};
    use std::thread;
    use std::time::Duration;

    struct CountingFuture {
        polls: Arc<Mutex<u32>>,
    }

    impl Future for CountingFuture {
        type Output = u32;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<u32> {
            *self.polls.lock().unwrap() += 1;
            Poll::Ready(7)
        }
    }

    #[test]
    fn ready_future_returns_its_value() {
        let polls = Arc::new(Mutex::new(0));

        let ready = block_on(future::ready(42));
        let counted = block_on(CountingFuture {
            polls: Arc::clone(&polls),
        });

        assert_eq!(ready, 42);
        assert_eq!(counted, 7);
        assert_eq!(*polls.lock().unwrap(), 1);
    }

    #[derive(Default)]
    struct Shared {
        value: Option<u32>,
        waker: Option<Waker>,
    }

    struct WaitForValue {
        shared: Arc<Mutex<Shared>>,
    }

    impl Future for WaitForValue {
        type Output = u32;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
            let mut shared = self.shared.lock().unwrap();
            if let Some(v) = shared.value {
                Poll::Ready(v)
            } else {
                shared.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }

    #[test]
    fn future_woken_from_another_thread_completes() {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let remote = Arc::clone(&shared);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let mut guard = remote.lock().unwrap();
            guard.value = Some(99);
            if let Some(waker) = guard.waker.take() {
                waker.wake();
            }
        });

        let result = block_on(WaitForValue { shared });

        handle.join().unwrap();
        assert_eq!(result, 99);
    }

    struct SelfWakeOnce {
        polled: bool,
    }

    impl Future for SelfWakeOnce {
        type Output = &'static str;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.polled {
                Poll::Ready("done")
            } else {
                self.polled = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    #[test]
    fn wake_before_park_does_not_deadlock() {
        let result = block_on(SelfWakeOnce { polled: false });

        assert_eq!(result, "done");
    }

    struct Panics;

    impl Future for Panics {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            panic!("future exploded");
        }
    }

    #[test]
    #[should_panic(expected = "future exploded")]
    fn panicking_future_propagates_panic() {
        block_on(Panics);
    }
}
