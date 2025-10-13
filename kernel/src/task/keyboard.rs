use core::pin::Pin;
use core::task::{Context, Poll};
use crate::{print, println};

use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use futures_util::{Stream, StreamExt};
use futures_util::task::AtomicWaker;
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
use crate::shell::{handle_terminal_command, TERMINAL_BUFFER};

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            println!("WARN: scancode queue full; dropping keyboard input");
        } else {
            WAKER.wake();
        }
    } else {
        println!("WARN: scancode queue uninitialized");
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(4096))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE
            .try_get()
            .expect("scancode queue not initialized");

        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(&cx.waker());
        match queue.pop() {
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}

pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Azerty,
        HandleControl::Ignore,
    );

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode('\x08') => {
                        TERMINAL_BUFFER.lock().pop();
                        print!("\x08");
                    },
                    DecodedKey::Unicode(c) => {
                        print!("{}", c);
                        if c == '\n' {
                            handle_terminal_command()
                        } else {
                            TERMINAL_BUFFER.lock().append_char(c);
                        }
                    },
                    DecodedKey::RawKey(_key) => {},
                }
            }
        }
    }
}
