//! Turning pictures into payloads and payloads into pictures.
//!
//! A desktop has no camera, so every account that arrives by QR code arrives as
//! an image file or a screenshot. Going the other way, an export has to become
//! a picture a phone can point at.

use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use qrcode::{Color, QrCode};

use super::ImportError;

/// Each QR module becomes this many pixels, which is large enough for a phone
/// to read off a screen without complaint.
const MODULE_PIXELS: u32 = 8;
/// The specification requires four blank modules around the code. Without them
/// scanners fail on a code that is otherwise perfectly formed.
const QUIET_MODULES: u32 = 4;

/// Flatten a picture to grey on a white ground.
///
/// `to_luma8` discards the alpha channel, and a fully transparent pixel is
/// almost always stored as transparent *black*. A code cropped onto a
/// transparent background therefore arrives as black on black — nothing at all.
/// Compositing over white first is what any viewer does before showing the
/// file, so it is also what the person who sent it saw.
fn grey_on_white(source: &DynamicImage) -> GrayImage {
    let rgba = source.to_rgba8();
    let mut grey = GrayImage::new(rgba.width(), rgba.height());

    for (x, y, pixel) in rgba.enumerate_pixels() {
        let [red, green, blue, alpha] = pixel.0;
        let over_white = |channel: u8| {
            ((channel as u32 * alpha as u32 + 255 * (255 - alpha as u32)) / 255) as f32
        };
        // The same Rec. 601 weighting `to_luma8` applies.
        let luma = 0.299 * over_white(red) + 0.587 * over_white(green) + 0.114 * over_white(blue);
        grey.put_pixel(x, y, Luma([luma.round() as u8]));
    }

    grey
}

/// Every code the detector can find and decode in one grey image.
fn decode_all(image: GrayImage) -> Vec<String> {
    let mut prepared = rqrr::PreparedImage::prepare(image);

    let mut found = Vec::new();
    for grid in prepared.detect_grids() {
        // A grid that will not decode is a damaged or partial code, not a
        // reason to discard the ones that did.
        if let Ok((_meta, content)) = grid.decode() {
            found.push(content);
        }
    }
    found
}

/// Find every QR code in an image and return what each one says.
///
/// An export of many accounts is several QR codes, and one screenshot may hold
/// all of them, so this returns a list rather than the first match.
pub fn read_qr_codes(image_bytes: &[u8]) -> Result<Vec<String>, ImportError> {
    let source = image::load_from_memory(image_bytes).map_err(|_| ImportError::NotAnImage)?;
    let grey = grey_on_white(&source);

    let found = decode_all(grey.clone());
    if !found.is_empty() {
        return Ok(found);
    }

    // Nothing the first way up. A screenshot taken in dark mode carries a light
    // code on a dark ground, and the detector does not try the reverse on its
    // own. This costs a second pass only when the first one found nothing.
    let mut flipped = grey;
    for pixel in flipped.pixels_mut() {
        pixel.0[0] = 255 - pixel.0[0];
    }

    let found = decode_all(flipped);
    if found.is_empty() {
        return Err(ImportError::NoQrCode);
    }
    Ok(found)
}

/// Render text as a PNG QR code.
///
/// The modules are drawn by hand rather than through `qrcode`'s own image
/// renderer: that feature pulls in its own version of `image`, and two versions
/// in the tree means the buffer types no longer match.
pub fn render_qr_png(text: &str) -> Result<Vec<u8>, ImportError> {
    let code = QrCode::new(text.as_bytes()).map_err(|_| ImportError::QrTooLarge)?;

    let modules = code.to_colors();
    let width = code.width() as u32;
    let side = (width + QUIET_MODULES * 2) * MODULE_PIXELS;

    // White everywhere, then the dark modules painted in. Starting white is
    // what gives the quiet zone the specification requires.
    let mut image = image::GrayImage::from_pixel(side, side, Luma([255u8]));

    for (index, colour) in modules.iter().enumerate() {
        if *colour != Color::Dark {
            continue;
        }
        let module_x = (index as u32) % width + QUIET_MODULES;
        let module_y = (index as u32) / width + QUIET_MODULES;

        for y in 0..MODULE_PIXELS {
            for x in 0..MODULE_PIXELS {
                image.put_pixel(
                    module_x * MODULE_PIXELS + x,
                    module_y * MODULE_PIXELS + y,
                    Luma([0u8]),
                );
            }
        }
    }

    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|_| ImportError::QrTooLarge)?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_payload_and_reads_it_back() {
        // The strongest available test without a camera: render, then decode.
        let text = "otpauth://totp/GitHub:alice?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let png = render_qr_png(text).unwrap();
        assert_eq!(&png[1..4], b"PNG", "not a PNG");

        assert_eq!(read_qr_codes(&png).unwrap(), vec![text.to_owned()]);
    }

    #[test]
    fn round_trips_a_payload_long_enough_to_matter() {
        // A ten-account migration export runs to about a kilobyte. QR codes
        // have version limits, and silently truncating would lose accounts.
        let long = format!("otpauth-migration://offline?data={}", "A".repeat(900));
        let png = render_qr_png(&long).unwrap();
        assert_eq!(read_qr_codes(&png).unwrap(), vec![long]);
    }

    #[test]
    fn refuses_a_payload_too_large_for_any_qr_code() {
        // Better a clear refusal than a picture no phone can read.
        assert_eq!(
            render_qr_png(&"A".repeat(10_000)),
            Err(ImportError::QrTooLarge)
        );
    }

    #[test]
    fn reports_an_image_with_no_qr_code_in_it() {
        let blank = blank_png(200, 200);
        assert_eq!(read_qr_codes(&blank), Err(ImportError::NoQrCode));
    }

    #[test]
    fn reports_a_file_that_is_not_an_image() {
        assert_eq!(
            read_qr_codes(b"this is not a picture"),
            Err(ImportError::NotAnImage)
        );
    }

    #[test]
    fn leaves_the_quiet_zone_scanners_require() {
        // A code drawn edge to edge is well-formed and unreadable.
        let png = render_qr_png("hello").unwrap();
        let img = image::load_from_memory(&png).unwrap().to_luma8();
        let inset = QUIET_MODULES * MODULE_PIXELS - 1;
        assert_eq!(img.get_pixel(inset, inset).0[0], 255, "no quiet zone");
    }

    #[test]
    fn a_code_drawn_on_a_transparent_background_is_still_found() {
        // Cropping a QR code in an image editor commonly leaves the light
        // squares transparent rather than white. Dropping the alpha channel
        // turns those pixels black, and a black code on a black field is
        // nothing at all.
        let png = render_qr_png("otpauth://totp/Example:alice?secret=GEZDGNBVGY3TQOJQ").unwrap();
        assert!(
            read_qr_codes(&on_transparent(&png)).is_ok(),
            "lost to the alpha channel"
        );
    }

    #[test]
    fn a_light_code_on_a_dark_background_is_still_found() {
        // What a screenshot taken in dark mode looks like. The code is intact;
        // only the polarity is reversed.
        let png = render_qr_png("otpauth://totp/Example:alice?secret=GEZDGNBVGY3TQOJQ").unwrap();
        assert!(
            read_qr_codes(&inverted(&png)).is_ok(),
            "not tried the other way up"
        );
    }

    #[test]
    fn flattening_leaves_an_opaque_black_and_white_picture_untouched() {
        // The export path renders Luma8 and reads it straight back, so the
        // flattening step must be exactly neutral on an opaque picture. A
        // rounding slip here would move every module a shade and is the kind of
        // thing that decodes fine until one day it does not.
        let mut checks = image::RgbaImage::new(2, 1);
        checks.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        checks.put_pixel(1, 0, image::Rgba([255, 255, 255, 255]));

        let grey = grey_on_white(&image::DynamicImage::ImageRgba8(checks));

        assert_eq!(grey.get_pixel(0, 0).0[0], 0, "black drifted");
        assert_eq!(grey.get_pixel(1, 0).0[0], 255, "white drifted");
    }

    /// Redraw a rendered code with its light squares fully transparent.
    fn on_transparent(png: &[u8]) -> Vec<u8> {
        let grey = image::load_from_memory(png).unwrap().to_luma8();
        let mut out = image::RgbaImage::new(grey.width(), grey.height());
        for (x, y, pixel) in grey.enumerate_pixels() {
            let rgba = if pixel.0[0] < 128 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            };
            out.put_pixel(x, y, rgba);
        }
        encode(image::DynamicImage::ImageRgba8(out))
    }

    /// Redraw a rendered code with its tones swapped.
    fn inverted(png: &[u8]) -> Vec<u8> {
        let mut grey = image::load_from_memory(png).unwrap().to_luma8();
        for pixel in grey.pixels_mut() {
            pixel.0[0] = 255 - pixel.0[0];
        }
        encode(image::DynamicImage::ImageLuma8(grey))
    }

    fn encode(image: image::DynamicImage) -> Vec<u8> {
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn blank_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::GrayImage::from_pixel(width, height, Luma([255u8]));
        let mut out = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }
}
