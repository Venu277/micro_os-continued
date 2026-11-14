use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

static KEYBOARD_WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// Called by the keyboard ISR when a new scancode is placed into the queue.
pub(crate) fn wake_keyboard_task() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(waker) = KEYBOARD_WAKER.lock().take() {
            waker.wake();
        }
    });
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        ScancodeStream { _private: () }
    }

    /// Wait for the next scancode asynchronously without busy waiting.
    pub async fn next_scancode(&self) -> u8 {
        struct NextScancode;
        impl Future for NextScancode {
            type Output = u8;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if let Some(scancode) = crate::interrupts::pop_scancode() {
                    Poll::Ready(scancode)
                } else {
                    let mut waker_guard = KEYBOARD_WAKER.lock();
                    *waker_guard = Some(cx.waker().clone());
                    // Re-check to prevent race condition between check and waker store
                    if let Some(scancode) = crate::interrupts::pop_scancode() {
                        *waker_guard = None;
                        Poll::Ready(scancode)
                    } else {
                        Poll::Pending
                    }
                }
            }
        }
        NextScancode.await
    }
}
