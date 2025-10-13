use alloc::string::String;
use lazy_static::lazy_static;
use spin::Mutex;
use crate::{print, println};

pub mod text;

lazy_static!(
    pub static ref TERMINAL_BUFFER: Mutex<TerminalBuffer> = Mutex::new(TerminalBuffer::new());
);

pub struct TerminalBuffer {
    buffer: String,
}

impl TerminalBuffer {
    fn new() -> Self {
        TerminalBuffer {
            buffer: String::new(),
        }
    }

    pub fn flush(&mut self) {
        self.buffer.clear();
    }

    pub fn append(&mut self, s: &str) {
        self.buffer.push_str(s);
    }

    pub fn pop(&mut self) {
        self.buffer.pop();
    }

    pub fn append_char(&mut self, c: char) {
        self.buffer.push(c);
    }

    pub fn get_command(&self) -> Option<String> {
        Some(self.buffer.clone())
    }
}

pub fn handle_terminal_command() {
    let mut buffer = TERMINAL_BUFFER.lock();

    if let Some(command) = buffer.get_command() {
        buffer.flush();

        match command.as_ref() {
            "" => {},
            "shutdown" => shutdown(),
            "help" => help_text(),
            _ => println!("Unknown command: {}, type help for a list of available commands.", command),
        }
    }
}

fn shutdown() {
    use x86_64::instructions::port::Port;
    x86_64::instructions::interrupts::disable();
    println!("Shutting down...");

    let mut port = Port::new(0xf4);

    unsafe {
        port.write(0u32);
    }
}

fn help_text() {
    println!("List of available commands:\n");
    println!(" - shutdown: shuts down the OS.");
    println!(" - help: display this message.");
    print!("\n")
}
