use image::{Frame, Rgba, RgbaImage};

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut result = kind.to_vec();
    result.extend_from_slice(&(data.len() as u32).to_le_bytes());
    result.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        result.push(0);
    }
    result
}

#[test]
fn detects_multiple_frames_without_mistaking_single_frames() {
    let dir = tempfile::tempdir().unwrap();
    for count in [1, 2] {
        let gif = dir.path().join(format!("{count}.gif"));
        let mut encoder = image::codecs::gif::GifEncoder::new(std::fs::File::create(&gif).unwrap());
        for value in 0..count {
            encoder
                .encode_frame(Frame::new(RgbaImage::from_pixel(
                    2,
                    2,
                    Rgba([value, 0, 0, 255]),
                )))
                .unwrap();
        }
        drop(encoder);
        assert_eq!(core_image::io::inspect_animation(&gif).unwrap(), count > 1);
        let mut static_webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut static_webp)
            .encode(&[100; 12], 2, 2, image::ExtendedColorType::Rgb8)
            .unwrap();
        let webp = dir.path().join(format!("{count}.webp"));
        let mut body = chunk(b"VP8X", &[2, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
        body.extend(chunk(b"ANIM", &[0; 6]));
        for _ in 0..count {
            let mut frame = vec![0; 16];
            frame[6] = 1;
            frame[9] = 1;
            frame[12] = 10;
            frame[15] = 2;
            frame.extend_from_slice(&static_webp[12..]);
            body.extend(chunk(b"ANMF", &frame));
        }
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend(body);
        std::fs::write(&webp, bytes).unwrap();
        assert_eq!(core_image::io::inspect_animation(&webp).unwrap(), count > 1);
        assert_eq!(core_image::load_image(&webp).unwrap().width(), 2);
        let static_path = dir.path().join("static.webp");
        std::fs::write(&static_path, &static_webp).unwrap();
        assert!(!core_image::io::inspect_animation(&static_path).unwrap());
    }
}
