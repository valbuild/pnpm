//! S4 reactor: pnpm's CLI as a poll-driven future, no threads, no blocking waits.
//!
//! JS host protocol:
//!   alloc(len) -> ptr        scratch memory for argv
//!   start(ptr, len)          argv as NUL-separated UTF-8 (argv[0] included)
//!   poll() -> 0 pending | 1 ok | 2 error
//!   complete(id)             a host operation settled; the caller then polls
use std::{
    cell::RefCell,
    ffi::OsString,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

type Root = Pin<Box<dyn Future<Output = miette::Result<()>>>>;

struct State {
    runtime: tokio::runtime::Runtime,
    root: Option<Root>,
    outcome: Option<i32>,
}

thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// # Safety
/// `pointer..pointer+len` must be a buffer obtained from [`alloc`] and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start(pointer: *mut u8, len: usize) {
    let bytes = unsafe { Vec::from_raw_parts(pointer, len, len) };
    let argv: Vec<OsString> = String::from_utf8(bytes)
        .expect("argv is UTF-8")
        .split('\0')
        .map(OsString::from)
        .collect();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("tokio current-thread runtime");
    let root: Root = Box::pin(pnpm_cli::run_async(argv));
    STATE.with(|state| {
        *state.borrow_mut() = Some(State { runtime, root: Some(root), outcome: None });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn poll() -> i32 {
    STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let state = guard.as_mut().expect("start() first");
        if let Some(outcome) = state.outcome {
            return outcome;
        }
        let mut root = state.root.take().expect("root future");
        let mut result = None;
        state.runtime.block_on(async {
            let mut context = Context::from_waker(Waker::noop());
            if let Poll::Ready(value) = root.as_mut().poll(&mut context) {
                result = Some(value);
            }
            // Let tasks spawned by the command make progress before returning to the host.
            for _ in 0..64 {
                tokio::task::yield_now().await;
            }
        });
        match result {
            None => {
                state.root = Some(root);
                0
            }
            Some(Ok(())) => {
                state.outcome = Some(1);
                1
            }
            Some(Err(error)) => {
                eprintln!("Error: {error:?}");
                state.outcome = Some(2);
                2
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn complete(identifier: u32) {
    pnpm_wasm_host::notify_completion(identifier);
}
