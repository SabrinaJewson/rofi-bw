pub(crate) fn poll_future_once<F: Future>(future: F) -> Option<F::Output> {
    pin!(future);
    let waker = NOOP_WAKER;
    let cx = &mut task::Context::from_waker(&waker);
    match future.poll(cx) {
        Poll::Ready(val) => Some(val),
        Poll::Pending => None,
    }
}

use noop_waker::NOOP_WAKER;
mod noop_waker {
    pub(crate) const NOOP_WAKER: Waker = unsafe { mem::transmute(RAW) };
    const RAW: RawWaker = RawWaker::new(ptr::null(), &VTABLE);
    const VTABLE: RawWakerVTable = RawWakerVTable::new(|_| RAW, |_| {}, |_| {}, |_| {});

    use std::mem;
    use std::ptr;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;
}

use std::future::Future;
use std::task;
use std::task::Poll;
use tokio::pin;
