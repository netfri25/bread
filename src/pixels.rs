use std::fs::File;
use std::os::unix::prelude::{AsFd as _, BorrowedFd};

use memmap::{MmapMut, MmapOptions};

// in the ARGB format
pub struct Pixels {
    file: File,
    mmap: MmapMut,
    width: u32,
    height: u32,
}

impl Pixels {
    pub fn new(width: u32, height: u32) -> Self {
        let stride = width as usize * 4;
        let len = stride * height as usize;

        // create a new file in the specified size and fill with 0s.
        // this will create an empty transparent buffer, which is fine since the first draw doesn't
        // really matter that much
        let file = tempfile::tempfile().expect("tmp file is mandatory");
        file.set_len(len as u64).expect("set len failed");

        // nothing to worry about :)
        let mmap = unsafe { MmapOptions::new().len(len).map_mut(&file).unwrap() };

        Self {
            file,
            mmap,
            width,
            height,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn stride(&self) -> u32 {
        self.width() * 4
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> u32 {
        self.height() * self.stride()
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }

    pub fn set(&mut self, x: u32, y: u32, color: Color) -> bool {
        if x >= self.width() || y >= self.height() {
            return false;
        }

        let index = x * 4 + y * self.stride();
        let index = index as usize;

        self.mmap[index..index + 4].copy_from_slice(&color.as_argb());

        true
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn as_argb(&self) -> [u8; 4] {
        [self.b, self.g, self.r, self.a]
    }
}

