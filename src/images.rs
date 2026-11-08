//! Bounded local raster loading. No URI APIs or extensible/vector decoders.
use gtk::{gdk, gio, glib, prelude::*};
use mdview::document::LocalTarget;
use std::{
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PIXELS: i64 = 16_000_000;
// Bound concurrent decoder allocations, even for documents with many images.
static DECODE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn dimensions(data: &[u8], kind: &str) -> Option<(u32, u32)> {
    fn be(bytes: &[u8]) -> Option<u32> {
        Some(u32::from_be_bytes(bytes.try_into().ok()?))
    }
    fn le(bytes: &[u8]) -> Option<u32> {
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }
    fn short(bytes: &[u8]) -> Option<u32> {
        Some(u16::from_le_bytes(bytes.try_into().ok()?) as u32)
    }
    match kind {
        "png" => {
            if data.get(12..16)? != b"IHDR" {
                return None;
            }
            Some((be(data.get(16..20)?)?, be(data.get(20..24)?)?))
        }
        "gif" => Some((short(data.get(6..8)?)?, short(data.get(8..10)?)?)),
        "bmp" => {
            if le(data.get(14..18)?)? == 12 {
                Some((short(data.get(18..20)?)?, short(data.get(20..22)?)?))
            } else {
                let width = le(data.get(18..22)?)? as i32;
                let height = le(data.get(22..26)?)? as i32;
                Some((u32::try_from(width).ok()?, height.unsigned_abs()))
            }
        }
        "pnm" => {
            let mut rest = data.get(2..)?;
            let mut values = Vec::new();
            while values.len() < 2 {
                while rest.first()?.is_ascii_whitespace() {
                    rest = &rest[1..];
                }
                if rest[0] == b'#' {
                    rest = rest.get(rest.iter().position(|b| *b == b'\n')? + 1..)?;
                    continue;
                }
                let end = rest.iter().position(|b| !b.is_ascii_digit())?;
                values.push(std::str::from_utf8(&rest[..end]).ok()?.parse().ok()?);
                rest = &rest[end..];
            }
            Some((values[0], values[1]))
        }
        "jpeg" => {
            let mut offset = 2;
            while offset < data.len() {
                if *data.get(offset)? != 0xff {
                    return None;
                }
                while *data.get(offset)? == 0xff {
                    offset += 1;
                }
                let marker = *data.get(offset)?;
                offset += 1;
                if matches!(marker, 0xd9 | 0xda) {
                    return None;
                }
                if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                    continue;
                }
                let length =
                    u16::from_be_bytes(data.get(offset..offset + 2)?.try_into().ok()?) as usize;
                if length < 2 {
                    return None;
                }
                if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                    let height =
                        u16::from_be_bytes(data.get(offset + 3..offset + 5)?.try_into().ok()?)
                            as u32;
                    let width =
                        u16::from_be_bytes(data.get(offset + 5..offset + 7)?.try_into().ok()?)
                            as u32;
                    return Some((width, height));
                }
                offset += length;
            }
            None
        }
        _ => None,
    }
}

// Reject animations before the decoder can retain an unbounded frame history.
fn validate_gif(data: &[u8]) -> Result<(), String> {
    fn scan(data: &[u8]) -> Option<()> {
        let packed = *data.get(10)?;
        let mut offset = 13
            + if packed & 0x80 != 0 {
                3 * (1 << ((packed & 7) + 1))
            } else {
                0
            };
        let mut frames = 0;
        loop {
            let marker = *data.get(offset)?;
            offset += 1;
            match marker {
                0x3b => return (frames == 1).then_some(()),
                0x21 => {
                    offset += 1;
                }
                0x2c => {
                    frames += 1;
                    if frames > 1 {
                        return None;
                    }
                    let desc = data.get(offset..offset + 9)?;
                    let w = u16::from_le_bytes(desc[4..6].try_into().ok()?) as u32;
                    let h = u16::from_le_bytes(desc[6..8].try_into().ok()?) as u32;
                    validate_dimensions(w, h).ok()?;
                    offset += 9;
                    if desc[8] & 0x80 != 0 {
                        offset += 3 * (1 << ((desc[8] & 7) + 1));
                    }
                    offset += 1; // LZW minimum code size
                }
                _ => return None,
            }
            loop {
                let length = *data.get(offset)? as usize;
                offset += 1;
                if length == 0 {
                    break;
                }
                offset += length;
                if offset > data.len() {
                    return None;
                }
            }
        }
    }
    scan(data).ok_or_else(|| "Animated or malformed GIF is unsupported".into())
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || i64::from(width) * i64::from(height) > MAX_PIXELS
    {
        Err("Image exceeds 16 million pixels or dimension limit".into())
    } else {
        Ok(())
    }
}

pub use mdview::navigation::decode_component;

pub fn local_path(base: &Path, source: &LocalTarget) -> Option<PathBuf> {
    // The decoded spelling remains a filesystem path, even if it resembles a URI.
    Some(base.join(decode_component(&source.path)?))
}

struct Pixels {
    data: Vec<u8>,
    width: i32,
    height: i32,
    stride: usize,
    alpha: bool,
}

fn load(path: &Path) -> Result<Pixels, String> {
    let _guard = DECODE_LOCK
        .lock()
        .map_err(|_| "Image decoder unavailable")?;
    // O_NONBLOCK prevents a raced-in FIFO from occupying a worker indefinitely.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o4000)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err("Not a regular image or exceeds 8 MiB".into());
    }
    let mut data = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Image exceeds 8 MiB".into());
    }
    let kind = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if data.starts_with(b"\xff\xd8\xff") {
        "jpeg"
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        "gif"
    } else if data.starts_with(b"BM") {
        "bmp"
    } else if data.len() >= 3
        && data[0] == b'P'
        && (b'1'..=b'6').contains(&data[1])
        && data[2].is_ascii_whitespace()
    {
        "pnm"
    } else {
        return Err("Unsupported image format (only PNG, JPEG, GIF, BMP and PNM)".into());
    };
    // Check the source pixel count before the decoder can allocate pixels.
    // Width/height and decoder byte-allocation limits are applied below too.
    let (width, height) = dimensions(&data, kind).ok_or("Malformed image header")?;
    validate_dimensions(width, height)?;
    if kind == "gif" {
        validate_gif(&data)?;
    }
    let format = match kind {
        "png" => image::ImageFormat::Png,
        "jpeg" => image::ImageFormat::Jpeg,
        "gif" => image::ImageFormat::Gif,
        "bmp" => image::ImageFormat::Bmp,
        "pnm" => image::ImageFormat::Pnm,
        _ => return Err("Unsupported image format".into()),
    };
    // Force the already validated raster format. No extensible platform
    // loader, URI resolution or decoder helper process is involved.
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    // Allow 16-bit intermediate pixels as well as the final 8-bit buffer.
    limits.max_alloc = Some(MAX_PIXELS as u64 * 16);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| error.to_string())?;
    validate_dimensions(decoded.width(), decoded.height())?;
    let scale = (1024.0 / f64::from(decoded.width()))
        .min(768.0 / f64::from(decoded.height()))
        .min(1.0);
    let decoded = if scale < 1.0 {
        decoded.resize_exact(
            (f64::from(decoded.width()) * scale).max(1.0) as u32,
            (f64::from(decoded.height()) * scale).max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        decoded
    };
    let (width, height) = (decoded.width() as i32, decoded.height() as i32);
    let alpha = decoded.color().has_alpha();
    let data = if alpha {
        decoded.into_rgba8().into_raw()
    } else {
        decoded.into_rgb8().into_raw()
    };
    Ok(Pixels {
        data,
        width,
        height,
        stride: width as usize * if alpha { 4 } else { 3 },
        alpha,
    })
}

struct ImageJob {
    path: PathBuf,
    container: glib::WeakRef<gtk::Box>,
    label: glib::WeakRef<gtk::Label>,
    alt: String,
}

#[derive(Default)]
struct ImageQueue {
    pending: std::collections::VecDeque<ImageJob>,
    busy: bool,
    #[cfg(test)]
    started: usize,
}

thread_local! {
    static IMAGE_QUEUE: std::cell::RefCell<ImageQueue> = std::cell::RefCell::default();
}

fn start_next_image() {
    let job = IMAGE_QUEUE.with_borrow_mut(|queue| {
        if queue.busy {
            return None;
        }
        while let Some(job) = queue.pending.pop_front() {
            if job.container.upgrade().is_none() {
                continue;
            }
            queue.busy = true;
            #[cfg(test)]
            {
                queue.started += 1;
            }
            return Some(job);
        }
        None
    });
    let Some(ImageJob {
        path,
        container,
        label,
        alt,
    }) = job
    else {
        return;
    };
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || load(&path)).await;
        if let (Some(container), Some(label)) = (container.upgrade(), label.upgrade()) {
            match result {
                Ok(Ok(pixels)) => {
                    let bytes = glib::Bytes::from_owned(pixels.data);
                    let texture = gdk::MemoryTexture::new(
                        pixels.width,
                        pixels.height,
                        if pixels.alpha {
                            gdk::MemoryFormat::R8g8b8a8
                        } else {
                            gdk::MemoryFormat::R8g8b8
                        },
                        &bytes,
                        pixels.stride,
                    );
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(true);
                    picture.set_size_request(1, pixels.height.clamp(96, 480));
                    picture.set_accessible_role(gtk::AccessibleRole::Img);
                    picture.update_property(&[gtk::accessible::Property::Label(&alt)]);
                    container.remove(&label);
                    container.append(&picture);
                }
                Ok(Err(error)) => label.set_text(&format!("[Image unavailable: {alt} — {error}]")),
                Err(_) => label.set_text(&format!("[Image unavailable: {alt}]")),
            }
        }
        IMAGE_QUEUE.with_borrow_mut(|queue| queue.busy = false);
        start_next_image();
    });
}

pub fn widget(base: &Path, source: Option<&LocalTarget>, alt: &str) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::new(Some(&format!("[Image blocked or unavailable: {alt}]")));
    label.set_wrap(true);
    container.append(&label);
    let Some(path) = source.and_then(|s| local_path(base, s)) else {
        return container;
    };
    label.set_text(&format!("[Loading image: {alt}]"));
    IMAGE_QUEUE.with_borrow_mut(|queue| {
        queue.pending.push_back(ImageJob {
            path,
            container: container.downgrade(),
            label: label.downgrade(),
            alt: alt.to_owned(),
        })
    });
    start_next_image();
    container
}

/// Runs on the existing GTK test thread.
#[cfg(test)]
pub fn verify_loading_queue() {
    let context = glib::MainContext::default();
    let wait = |condition: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !condition() {
            assert!(
                std::time::Instant::now() < deadline,
                "image queue did not settle"
            );
            context.block_on(glib::timeout_future(std::time::Duration::from_millis(10)));
        }
    };
    wait(&|| IMAGE_QUEUE.with_borrow(|queue| !queue.busy && queue.pending.is_empty()));
    let guard = DECODE_LOCK.lock().unwrap();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let source = LocalTarget {
        path: "assets/sample.ppm".into(),
        fragment: None,
    };
    let first = widget(&base, Some(&source), "first");
    let started = IMAGE_QUEUE.with_borrow(|queue| queue.started);
    for _ in 0..50 {
        let retired = widget(&base, Some(&source), "retired");
        let weak = retired.downgrade();
        drop(retired);
        assert!(
            weak.upgrade().is_none(),
            "queued work must not retain widgets"
        );
    }
    let last = widget(&base, Some(&source), "last");
    assert_eq!(IMAGE_QUEUE.with_borrow(|queue| queue.started), started);
    drop(guard);
    wait(&|| IMAGE_QUEUE.with_borrow(|queue| !queue.busy && queue.pending.is_empty()));
    assert_eq!(
        IMAGE_QUEUE.with_borrow(|queue| queue.started),
        started + 1,
        "only the surviving queued image should be decoded"
    );
    assert!(
        first
            .first_child()
            .is_some_and(|child| child.is::<gtk::Picture>())
    );
    assert!(
        last.first_child()
            .is_some_and(|child| child.is::<gtk::Picture>())
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_rasters_and_paths() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let pixels = load(&base.join("assets/sample.ppm")).unwrap();
        assert_eq!((pixels.width, pixels.height), (4, 4));
        assert!(load(&base.join("basic.md")).is_err());
        assert!(load(&base.join("assets/missing.png")).is_err());
        assert!(validate_dimensions(4001, 4000).is_err());
        assert!(validate_dimensions(16385, 1).is_err());
        assert!(validate_dimensions(0, 1).is_err());
        let mut gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00".to_vec();
        let frame = b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x00";
        gif.extend_from_slice(frame);
        gif.push(0x3b);
        assert!(validate_gif(&gif).is_ok());
        gif.pop();
        gif.extend_from_slice(frame);
        gif.push(0x3b);
        assert!(validate_gif(&gif).is_err());
        assert_eq!(
            dimensions(b"P3\n# a comment\n4 5\n255\n", "pnm"),
            Some((4, 5))
        );
        assert_eq!(
            dimensions(b"\xff\xd8\xff\xc0\x00\x11\x08\x00\x05\x00\x04", "jpeg"),
            Some((4, 5))
        );
        let temp = std::env::temp_dir().join(format!("mdview-images-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        for format in [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::Gif,
            image::ImageFormat::Bmp,
            image::ImageFormat::Pnm,
        ] {
            let raster = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                2,
                2,
                image::Rgb([255, 0, 0]),
            ));
            let mut encoded = std::io::Cursor::new(Vec::new());
            if format == image::ImageFormat::Pnm {
                // The general encoder defaults to PAM/P7, outside our P1–P6
                // policy. Exercise an independently specified binary PPM.
                encoded.get_mut().extend_from_slice(b"P6\n2 2\n255\n");
                encoded.get_mut().extend_from_slice(&[255, 0, 0].repeat(4));
            } else {
                raster.write_to(&mut encoded, format).unwrap();
            }
            let bytes = encoded.into_inner();
            let path = temp.join(format!("{format:?}.raster"));
            std::fs::write(&path, &bytes).unwrap();
            let pixels = load(&path).unwrap();
            assert_eq!((pixels.width, pixels.height), (2, 2));
            assert!(pixels.data[0] >= 240 && pixels.data[1] < 16 && pixels.data[2] < 16);
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        let alpha_path = temp.join("alpha.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([32, 64, 128, 128]))
            .save(&alpha_path)
            .unwrap();
        let alpha = load(&alpha_path).unwrap();
        assert!(alpha.alpha);
        assert_eq!(&alpha.data[..4], &[32, 64, 128, 128]);
        let scaled_path = temp.join("scaled.png");
        image::RgbImage::from_pixel(2048, 512, image::Rgb([255, 0, 0]))
            .save(&scaled_path)
            .unwrap();
        let scaled = load(&scaled_path).unwrap();
        assert_eq!((scaled.width, scaled.height), (1024, 256));
        for (name, bytes, expected) in [
            ("remote.svg", b"<svg xmlns='http://www.w3.org/2000/svg'><image href='https://example.org/private.png'/></svg>".as_slice(), "Unsupported"),
            ("huge.ppm", b"P3\n100000 100000\n255\n".as_slice(), "exceeds"),
            ("broken.png", b"\x89PNG\r\n\x1a\n".as_slice(), "Malformed"),
        ] {
            let path = temp.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert!(load(&path).err().unwrap().contains(expected));
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "loading changed source");
        }
        let path = temp.join("too-large.png");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(load(&path).err().unwrap().contains("8 MiB"));
        assert!(load(&temp).is_err());
        std::fs::remove_dir_all(temp).unwrap();
        assert_eq!(
            decode_component("caf%C3%A9%2520.png").unwrap(),
            "café%20.png"
        );
        assert!(decode_component("%00").is_none());
        let target = LocalTarget {
            path: "https%3A%2F%2Fexample.png".into(),
            fragment: None,
        };
        assert_eq!(
            local_path(&base, &target).unwrap(),
            base.join("https://example.png")
        );
    }
}
