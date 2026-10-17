//! CPU-rendered dma-bufs allocated through Mesa GBM. GBM is required on
//! newer drivers (including Intel Xe), where DRM dumb-buffer ioctls may be
//! forbidden even though the render node itself is accessible.

use std::fs::File;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use gbm::{BufferObject, BufferObjectFlags, Device, Format};

pub const DRM_FORMAT_ARGB8888: u32 = 0x34325241;

fn render_node() -> std::io::Result<File> {
    let entries = std::fs::read_dir("/dev/dri")
        .map_err(|e| std::io::Error::new(e.kind(), format!("read /dev/dri: {e}")))?;
    let mut nodes = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("renderD"))
        .map(|e| e.path())
        .collect::<Vec<_>>();
    nodes.sort();
    let mut last_error = None;
    for path in nodes {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => return Ok(file),
            Err(e) => last_error = Some((path, e)),
        }
    }
    match last_error {
        Some((path, e)) => Err(std::io::Error::new(
            e.kind(),
            format!("open {}: {e}", path.display()),
        )),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no /dev/dri/renderD* node",
        )),
    }
}

pub struct DmaAlloc {
    // Fields drop in declaration order: destroy the BO before its GBM device.
    bo: BufferObject<()>,
    _device: Device<File>,
    dma: OwnedFd,
    width: u32,
    height: u32,
    stride: usize,
    modifier: u64,
}

impl DmaAlloc {
    pub fn new(width: u32, height: u32) -> std::io::Result<Self> {
        let device = Device::new(render_node()?)
            .map_err(|e| std::io::Error::new(e.kind(), format!("create GBM device: {e}")))?;
        let flags = BufferObjectFlags::RENDERING | BufferObjectFlags::LINEAR;
        let bo = device
            .create_buffer_object::<()>(width, height, Format::Argb8888, flags)
            .map_err(|e| std::io::Error::new(e.kind(), format!("allocate GBM buffer: {e}")))?;
        if bo.plane_count() != 1 {
            return Err(std::io::Error::other(format!(
                "GBM returned {} planes for ARGB8888",
                bo.plane_count()
            )));
        }
        let stride = bo.stride() as usize;
        let modifier = bo.modifier().into();
        let dma = bo
            .fd()
            .map_err(|e| std::io::Error::other(format!("export GBM dma-buf: {e}")))?;
        Ok(Self {
            bo,
            _device: device,
            dma,
            width,
            height,
            stride,
            modifier,
        })
    }

    pub fn dma_fd(&self) -> BorrowedFd<'_> {
        self.dma.as_fd()
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn modifier(&self) -> u64 {
        self.modifier
    }

    pub fn write_rgba(&mut self, rgba: &[u8]) -> std::io::Result<()> {
        let expected = self.width as usize * self.height as usize * 4;
        if rgba.len() != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("RGBA frame is {} bytes, expected {expected}", rgba.len()),
            ));
        }
        let (width, height) = (self.width, self.height);
        self.bo.map_mut(0, 0, width, height, |mapped| {
            let stride = mapped.stride() as usize;
            pack_argb8888(mapped.buffer_mut(), stride, rgba, width, height);
        })
    }
}

/// Reorder premultiplied RGBA into little-endian ARGB8888 (BGRA bytes).
/// Alpha and color values must not be rescaled: both tiny-skia and Wayland
/// use premultiplied alpha.
pub fn pack_argb8888(dst: &mut [u8], dst_stride: usize, src: &[u8], width: u32, height: u32) {
    for y in 0..height as usize {
        let drow = &mut dst[y * dst_stride..y * dst_stride + width as usize * 4];
        let srow = &src[y * width as usize * 4..(y + 1) * width as usize * 4];
        for (d, s) in drow.chunks_exact_mut(4).zip(srow.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbm_alloc_works_or_skips_without_gpu_access() {
        let mut alloc = match DmaAlloc::new(64, 64) {
            Ok(alloc) => alloc,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                eprintln!("SKIP: no accessible render node ({e})");
                return;
            }
            Err(e) => panic!("unexpected GBM error: {e}"),
        };
        assert!(alloc.stride() >= 64 * 4);
        alloc.write_rgba(&vec![0; 64 * 64 * 4]).unwrap();
    }

    #[test]
    fn argb_pack_swaps_r_and_b() {
        let mut dst = [0u8; 8];
        // A translucent premultiplied pixel stays premultiplied.
        pack_argb8888(&mut dst, 8, &[0x11, 0x22, 0x33, 0x44], 1, 1);
        assert_eq!(&dst[..4], &[0x33, 0x22, 0x11, 0x44]);
    }
}
