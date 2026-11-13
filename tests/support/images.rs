use image::{Rgba, RgbaImage};

pub fn mixed_atlas(order: [u32; 3]) -> RgbaImage {
    RgbaImage::from_fn(192, 64, |x, y| {
        let frame = order[usize::try_from(x / 64).unwrap()];
        let local_x = x % 64;
        let value = match frame {
            0 => {
                if (local_x / 16 + y / 16) % 2 == 0 {
                    32
                } else {
                    224
                }
            }
            1 => u8::try_from(local_x * 4).unwrap(),
            _ => {
                if (local_x + y) % 2 == 0 {
                    32
                } else {
                    224
                }
            }
        };
        Rgba([value, value, value, 255])
    })
}

pub fn unstructured() -> RgbaImage {
    let mut state = 0x1234_5678_u32;
    RgbaImage::from_fn(96, 64, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let value = 112 + u8::try_from(state % 32).unwrap();
        Rgba([value, value, value, 255])
    })
}

pub fn flat_graphic() -> RgbaImage {
    RgbaImage::from_fn(64, 64, |x, y| {
        let value = if (x / 16 + y / 16) % 2 == 0 { 32 } else { 224 };
        Rgba([value, value, value, 255])
    })
}
