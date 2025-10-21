use std::os::unix::prelude::{AsFd as _, BorrowedFd};

use memfd::{Memfd, MemfdOptions};
use memmap2::{Advice, MmapMut, MmapOptions};

// in the ARGB format
pub struct Pixels {
    mfd: Memfd,
    mmap: MmapMut,
    width: u32,
}

impl Pixels {
    pub fn new(width: u32, height: u32) -> Self {
        let stride = width as usize * 4;
        let len = stride * height as usize;

        let mfd = MemfdOptions::new()
            .allow_sealing(true)
            .create("pixels")
            .expect("memfd create");

        mfd.as_file().set_len(len as u64).expect("set len");

        // nothing to worry about :)
        let mmap = unsafe {
            MmapOptions::new()
                .len(len)
                .no_reserve_swap()
                .populate()
                .map_mut(mfd.as_file())
                .expect("mmap")
        };

        mmap.advise(Advice::Random).expect("advice");

        Self { mfd, mmap, width }
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn stride(&self) -> u32 {
        self.width() * 4
    }

    pub fn height(&self) -> u32 {
        self.mmap.len() as u32 / self.stride()
    }

    pub fn size(&self) -> u32 {
        self.height() * self.stride()
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.mfd.as_file().as_fd()
    }

    pub fn clear(&mut self, color: Color) {
        let mut bytes = color.as_argb().into_iter().cycle();
        self.mmap.fill_with(|| bytes.next().unwrap_or_default());
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

    pub fn interpolate(self, other: Self, f: f32) -> Self {
        let f = f.clamp(0., 1.);

        let r = self.r as f32 * (1. - f) + other.r as f32 * f;
        let g = self.g as f32 * (1. - f) + other.g as f32 * f;
        let b = self.b as f32 * (1. - f) + other.b as f32 * f;
        let a = self.a as f32 * (1. - f) + other.a as f32 * f;

        let r = r.round().clamp(0., 255.) as u8;
        let g = g.round().clamp(0., 255.) as u8;
        let b = b.round().clamp(0., 255.) as u8;
        let a = a.round().clamp(0., 255.) as u8;

        Self { r, g, b, a }
    }
}
