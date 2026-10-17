//! Xcursor file writer. Port of `build.py::xcursor`.
//!
//! Format: header, table of contents, then ARGB32 chunks. Pixels are
//! premultiplied alpha — writing straight alpha fringes every soft edge
//! (see the comment in `build.py`).

/// One size frame: nominal size, hotspot, width/height, straight-alpha RGBA.
pub struct Frame {
    pub nominal: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn solid(nominal: u32, width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut px = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            px.extend_from_slice(&rgba);
        }
        Self {
            nominal,
            hot_x: 0,
            hot_y: 0,
            width,
            height,
            rgba: px,
        }
    }
}

fn u32le(v: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Encode frames as one Xcursor file. Mirrors `build.py::xcursor`.
pub fn encode(frames: &[Frame]) -> Vec<u8> {
    let n = frames.len() as u32;
    let mut blob = Vec::new();
    // magic 'Xcur', header len 16, version 0x10000, ntoc
    u32le(0x7275_6358, &mut blob);
    u32le(16, &mut blob);
    u32le(0x10000, &mut blob);
    u32le(n, &mut blob);

    let mut offset = 16 + n * 12;
    let mut toc = Vec::new();
    let mut chunks = Vec::new();
    for f in frames {
        assert_eq!(f.rgba.len(), (f.width * f.height * 4) as usize);
        let mut chunk = Vec::new();
        u32le(36, &mut chunk); // chunk header len
        u32le(0xfffd_0002, &mut chunk); // image type
        u32le(f.nominal, &mut chunk);
        u32le(1, &mut chunk); // version
        u32le(f.width, &mut chunk);
        u32le(f.height, &mut chunk);
        u32le(f.hot_x, &mut chunk);
        u32le(f.hot_y, &mut chunk);
        u32le(0, &mut chunk); // delay (0 = static cursor)
        for px in f.rgba.chunks_exact(4) {
            let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
            // Premultiply, exactly like build.py.
            let argb = (a << 24) | ((r * a / 255) << 16) | ((g * a / 255) << 8) | (b * a / 255);
            u32le(argb, &mut chunk);
        }
        let mut entry = Vec::new();
        u32le(0xfffd_0002, &mut entry);
        u32le(f.nominal, &mut entry);
        u32le(offset, &mut entry);
        offset += chunk.len() as u32;
        toc.push(entry);
        chunks.push(chunk);
    }
    for e in toc {
        blob.extend_from_slice(&e);
    }
    for c in chunks {
        blob.extend_from_slice(&c);
    }
    blob
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_premultiplication() {
        // 1x1 half-transparent red: premultiplied = (128, 128,0,0).
        let f = Frame {
            nominal: 24,
            hot_x: 1,
            hot_y: 2,
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 128],
        };
        let b = encode(&[f]);
        assert_eq!(&b[0..4], b"Xcur");
        assert_eq!(u32::from_le_bytes(b[12..16].try_into().unwrap()), 1);
        // Pixel is the last 4 bytes: 0x80_80_00_00 LE.
        let px = u32::from_le_bytes(b[b.len() - 4..].try_into().unwrap());
        assert_eq!(px, (128 << 24) | (128 << 16));
    }

    #[test]
    fn multi_frame_offsets_monotonic() {
        let frames = vec![
            Frame::solid(24, 2, 2, [255, 255, 255, 255]),
            Frame::solid(32, 2, 2, [0, 0, 0, 0]),
        ];
        let b = encode(&frames);
        let off1 = u32::from_le_bytes(b[16 + 8..16 + 12].try_into().unwrap());
        let off2 = u32::from_le_bytes(b[28 + 8..28 + 12].try_into().unwrap());
        assert!(off2 > off1);
        assert_eq!(off1 as usize + (36 + 16), off2 as usize);
    }
}
