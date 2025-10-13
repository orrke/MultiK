
const PSF2_MAGIC: u32 = 0x864AB572;

const FONT_DATA: &[u8] = include_bytes!("../font/Tamsyn8x15b.psf");

lazy_static!(
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        text_color: 0xFFFFFFFF,
        background_color: 0xFF000000,
        font: load_psf2(FONT_DATA).expect("failed to load font"),
        buffer: Buffer::new(FRAMEBUFFER.lock().as_ref().unwrap()),
    });
);

use core::arch::asm;
use core::fmt;
use lazy_static::lazy_static;
use limine::framebuffer::Framebuffer;
use spin::Mutex;
use crate::FRAMEBUFFER_REQUEST;

pub static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Psf2Header {
    magic: u32,
    version: u32,
    headersize: u32,
    flags: u32,
    length: u32,
    charsize: u32,
    pub height: u32,
    pub width: u32,
}

#[derive(Debug)]
pub struct Psf2Font<'a> {
    pub header: Psf2Header,
    glyphs: &'a [u8],
}

impl<'a> Psf2Font<'a> {
    pub fn get_glyph(&self, ch: u32) -> Option<&'a [u8]> {
        if ch >= self.header.length {
            return None;
        }
        let start = (ch * self.header.charsize) as usize;
        let end = start + self.header.charsize as usize;
        Some(&self.glyphs[start..end])
    }
}

pub fn load_psf2(data: &[u8]) -> Option<Psf2Font<'_>> {
    use core::mem;
    if data.len() < mem::size_of::<Psf2Header>() {
        return None;
    }
    let header = unsafe { *(data.as_ptr() as *const Psf2Header) };
    if header.magic != PSF2_MAGIC {
        return None;
    }
    let glyphs = &data[header.headersize as usize ..
        (header.headersize + header.charsize * header.length) as usize];
    Some(Psf2Font { header, glyphs })
}

pub struct Buffer {
    start: usize,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: usize
}

impl Buffer {
    pub fn new(framebuffer: &Framebuffer) -> Buffer {
        Buffer {
            start: framebuffer.addr() as usize,
            width: framebuffer.width() as usize,
            height: framebuffer.height() as usize,
            pitch: framebuffer.pitch() as usize,
            bpp: framebuffer.bpp() as usize,
        }
    }

    pub fn place_pixel(&mut self, x: usize, y: usize, val: u32) {
        if let Some(ptr) = self.pixel_mut(x, y) {
            *ptr = val;
        }
    }

    pub fn pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut u32> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let offset = (y * self.pitch) + (x * (self.bpp / 8));

        unsafe {
            let pixel_ptr = (self.start + offset) as *mut u32;
            Some(&mut *pixel_ptr)
        }
    }

    pub fn shift_upwards(&mut self, rows: usize) {
        let bytes_per_row = self.pitch; // Assuming pitch is bytes per row
        let offset = bytes_per_row * rows;

        // The number of bytes in the region we need to move.
        // This is the total size of the screen minus the number of rows we are scrolling.
        let copy_len = self.height * bytes_per_row - offset;

        // The destination pointer is the start of the framebuffer.
        let dest_ptr = self.start as *mut u8;
        // The source pointer is the start of the first row we want to move up.
        let src_ptr = (self.start + offset) as *const u8;

        // Use `ptr::copy` because the source and destination memory regions overlap.
        // `ptr::copy` is smart enough to handle this correctly (by copying backwards).
        unsafe {
            core::ptr::copy(src_ptr, dest_ptr, copy_len);
        }

        // Now, you still need to clear the newly exposed rows at the bottom.
        let clear_start_ptr = (self.start + copy_len) as *mut u8;
        let clear_len = offset; // The size of the scrolled region
        unsafe {
            core::ptr::write_bytes(clear_start_ptr, 0, clear_len);
        }
    }
}

pub struct Writer {
    column_position: usize,
    text_color: u32,
    background_color: u32,
    font: Psf2Font<'static>,
    buffer: Buffer,
}

impl Writer {
    pub fn write_char(&mut self, c: char) {
        match c {
            '\x08' => self.backspace(),
            '\n' => self.new_line(),
            c => {
                if self.column_position >= self.buffer.width {
                    self.new_line();
                }

                let glyph = self.font.get_glyph(c as u32).unwrap();

                let row_current = self.buffer.height - self.font.header.height as usize;
                let col_current = self.column_position;

                for (row_idx, row) in glyph.iter().enumerate() {
                    for col_idx in 0..self.font.header.width as usize {
                        if (row >> (7 - col_idx)) & 1 == 1 {
                            self.buffer.place_pixel(col_idx + col_current, row_idx + row_current, self.text_color);
                        } else {
                            self.buffer.place_pixel(col_idx + col_current, row_idx + row_current, self.background_color);
                        }
                    }
                }

                self.column_position += self.font.header.width as usize;
            },
        }
    }

    fn write_string(&mut self, string: &str) {
        for c in string.chars() {
            self.write_char(c);
        }
    }

    fn new_line(&mut self) {
        let font_height = self.font.header.height as usize;
        self.buffer.shift_upwards(font_height);

        self.clear_row();
        self.column_position = 0;
    }

    fn clear_row(&mut self) {
        for y in self.buffer.height - self.font.header.height as usize..self.buffer.height {
            for x in 0..self.buffer.width {
                self.buffer.place_pixel(x, y, self.background_color);
            }
        }
    }

    pub fn change_color(&mut self, color: u32) {
        self.text_color = color;
    }

    pub fn backspace(&mut self) {
        if self.column_position > 0 {
            self.column_position -= self.font.header.width as usize;
            self.write_char(' ');
            self.column_position -= self.font.header.width as usize;
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {crate::shell::text::_print(format_args!($($arg)*))};
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;

    #[allow(unused_unsafe)]
    unsafe {
        //asm!("cli");
        WRITER.lock().write_fmt(args).unwrap();
        //asm!("sti");
    }
}

pub fn init_framebuffer() {
    let framebuffer_response = FRAMEBUFFER_REQUEST.get_response().expect("Limine didn't respond to the framebuffer request");
    let framebuffer = framebuffer_response.framebuffers().next().expect("No framebuffers available");

    FRAMEBUFFER.lock().replace(framebuffer);
}
