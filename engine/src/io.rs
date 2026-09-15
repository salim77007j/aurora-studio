//! Import/export: PNG, JPEG, WebP, GIF, BMP, TIFF, SVG, OpenRaster (.ora), PSD (read + write).
use crate::blend::BlendMode;
use crate::layer::{Document, Layer, Node};
use std::io::{BufReader, Cursor, Read, Seek, Write};
use image::ImageEncoder;

pub fn detect_format(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    for (ext, fmt) in [
        ("png", "png"), ("jpg", "jpeg"), ("jpeg", "jpeg"), ("webp", "webp"), ("gif", "gif"),
        ("bmp", "bmp"), ("tif", "tiff"), ("tiff", "tiff"), ("svg", "svg"), ("ora", "ora"),
        ("psd", "psd"),
    ] {
        if lower.ends_with(ext) {
            return fmt;
        }
    }
    "png"
}

pub fn open(path: &str) -> Result<Document, String> {
    match detect_format(path) {
        "svg" => open_svg(path),
        "ora" => open_ora(path),
        "psd" => open_psd(path),
        f => open_raster(path, f),
    }
}

fn open_raster(path: &str, _fmt: &str) -> Result<Document, String> {
    let img = image::ImageReader::open(path).map_err(|e| format!("open: {e}"))?
        .with_guessed_format().map_err(|e| format!("format: {e}"))?
        .decode().map_err(|e| format!("decode: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let name = file_stem(path);
    let mut doc = Document::new(w, h, [0, 0, 0, 0], &name);
    let id = doc.alloc_id();
    let mut layer = Layer::new(id, "Background", w, h);
    layer.pixels.data.copy_from_slice(rgba.as_raw());
    doc.root.children.push(Node::Layer(layer));
    doc.active_id = id;
    doc.recomposite_all();
    Ok(doc)
}

fn open_svg(path: &str) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opt).map_err(|e| format!("svg parse: {e}"))?;
    let size = tree.size();
    let w = (size.width().ceil() as u32).max(1).min(8192);
    let h = (size.height().ceil() as u32).max(1).min(8192);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("svg raster alloc failed")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    let mut doc = Document::new(w, h, [0, 0, 0, 0], "SVG");
    let id = doc.alloc_id();
    let mut layer = Layer::new(id, "Vector", w, h);
    for (i, px) in pixmap.pixels().iter().enumerate() {
        let a = px.alpha() as u32;
        let un = |v: u8| ((v as u32 * 255 + a / 2) / a.max(1)).min(255) as u8;
        layer.pixels.data[i * 4..i * 4 + 4].copy_from_slice(&[un(px.red()), un(px.green()), un(px.blue()), px.alpha()]);
    }
    doc.root.children.push(Node::Layer(layer));
    doc.active_id = id;
    doc.recomposite_all();
    Ok(doc)
}

fn open_ora(path: &str) -> Result<Document, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| format!("zip: {e}"))?;
    let stack_xml = read_zip_entry(&mut zip, "stack.xml")?;
    let xml = String::from_utf8_lossy(&stack_xml).to_string();
    let w = extract_attr(&xml, "image", "w").and_then(|v| v.parse().ok()).ok_or("ora: bad w")?;
    let h = extract_attr(&xml, "image", "h").and_then(|v| v.parse().ok()).ok_or("ora: bad h")?;
    let mut doc = Document::new(w, h, [0, 0, 0, 0], &file_stem(path));
    let mut entries: Vec<(String, String)> = Vec::new();
    collect_ora_layers(&xml, &mut entries);
    entries.reverse(); // ORA lists top-first; engine stores bottom-first
    for (i, (src, name)) in entries.iter().enumerate() {
        let png = read_zip_entry(&mut zip, src)?;
        let img = image::load_from_memory(&png).map_err(|e| format!("ora layer: {e}"))?.to_rgba8();
        let id = doc.alloc_id();
        let nm = if name.is_empty() { format!("Layer {}", i + 1) } else { name.clone() };
        let mut layer = Layer::new(id, &nm, w, h);
        for y in 0..h.min(img.height()) {
            for x in 0..w.min(img.width()) {
                layer.pixels.set(x as i32, y as i32, img.get_pixel(x, y).0);
            }
        }
        doc.root.children.push(Node::Layer(layer));
    }
    if !doc.root.children.is_empty() {
        doc.active_id = doc.layer_ids()[doc.layer_ids().len() - 1];
    }
    doc.recomposite_all();
    Ok(doc)
}

fn read_zip_entry(zip: &mut zip::ZipArchive<BufReader<std::fs::File>>, name: &str) -> Result<Vec<u8>, String> {
    let mut f = zip.by_name(name).map_err(|e| format!("zip entry {name}: {e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| format!("read {name}: {e}"))?;
    Ok(buf)
}

fn collect_ora_layers(xml: &str, out: &mut Vec<(String, String)>) {
    let mut rest = xml;
    while let Some(pos) = rest.find('<') {
        let end = match rest[pos..].find('>') {
            Some(e) => pos + e,
            None => break,
        };
        let tag_full = &rest[pos + 1..end];
        let tag_name = tag_full.split_whitespace().next().unwrap_or("").trim_end_matches("/>").to_string();
        if tag_name == "layer" {
            let src = extract_attr_from_tag(tag_full, "src").unwrap_or_default();
            let name = extract_attr_from_tag(tag_full, "name").unwrap_or_default();
            out.push((src, name));
        }
        rest = &rest[end + 1..];
    }
}

fn extract_attr_from_tag(tag: &str, attr: &str) -> Option<String> {
    let pat = format!("{attr}=\"");
    let p = tag.find(&pat)? + pat.len();
    let e = tag[p..].find('"')?;
    Some(tag[p..p + e].to_string())
}

fn extract_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let tpat = format!("<{tag}");
    let tp = xml.find(&tpat)?;
    let tag_end = xml[tp..].find('>')? + tp;
    extract_attr_from_tag(&xml[tp..tag_end], attr)
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into())
}

pub struct ExportParams {
    pub path: String,
    pub format: &'static str,   // png jpeg webp gif bmp tiff svg ora psd
    pub quality: i32,           // 1..100 (jpeg)
    pub compression: i32,       // 0..9 (png)
    pub lossless: bool,         // reserved for webp-capable encoders
    pub layers_as_frames: bool, // gif
}

pub fn export(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    match p.format {
        "png" => export_png(doc, p),
        "jpeg" | "jpg" => export_jpeg(doc, p),
        "webp" => export_webp(doc, p),
        "gif" => export_gif(doc, p),
        "bmp" => export_bmp(doc, p),
        "tiff" => export_tiff(doc, p),
        "svg" => export_svg(doc, p),
        "ora" => export_ora(doc, p),
        "psd" => export_psd(doc, p),
        other => Err(format!("unknown format: {other}")),
    }
}

fn save(path: &str, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("write: {e}"))
}

fn export_png(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let flat = doc.flatten();
    let comp = p.compression.clamp(0, 9);
    let ctype = if comp <= 3 { image::codecs::png::CompressionType::Fast }
        else if comp >= 7 { image::codecs::png::CompressionType::Best }
        else { image::codecs::png::CompressionType::Default };
    let file = std::fs::File::create(&p.path).map_err(|e| format!("create: {e}"))?;
    let mut enc = image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file), ctype, image::codecs::png::FilterType::Adaptive,
    );
    enc.write_image(&flat.data, flat.w, flat.h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("png: {e}"))?;
    Ok(())
}

fn export_jpeg(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let flat = doc.flatten();
    // JPEG has no alpha: composite over white
    let n = (flat.w * flat.h) as usize;
    let mut raw = vec![0u8; n * 3];
    for i in 0..n {
        let a = flat.data[i * 4 + 3] as u32;
        for c in 0..3 {
            raw[i * 3 + c] = ((flat.data[i * 4 + c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        }
    }
    let file = std::fs::File::create(&p.path).map_err(|e| format!("create: {e}"))?;
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), p.quality.clamp(1, 100) as u8);
    enc.write_image(&raw, flat.w, flat.h, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("jpeg: {e}"))?;
    Ok(())
}

fn export_webp(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    // image's WebP encoder produces lossless WebP (the `lossless` flag is informational;
    // quality applies to JPEG).
    let flat = doc.flatten();
    let file = std::fs::File::create(&p.path).map_err(|e| format!("create: {e}"))?;
    let mut enc = image::codecs::webp::WebPEncoder::new_lossless(std::io::BufWriter::new(file));
    enc.write_image(&flat.data, flat.w, flat.h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("webp: {e}"))?;
    let _ = p;
    Ok(())
}

fn export_bmp(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let flat = doc.flatten();
    let img = image::RgbaImage::from_raw(flat.w, flat.h, flat.data.clone()).ok_or("bmp buf")?;
    img.save_with_format(&p.path, image::ImageFormat::Bmp).map_err(|e| format!("bmp: {e}"))?;
    Ok(())
}

fn export_tiff(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let flat = doc.flatten();
    let img = image::RgbaImage::from_raw(flat.w, flat.h, flat.data.clone()).ok_or("tiff buf")?;
    img.save_with_format(&p.path, image::ImageFormat::Tiff).map_err(|e| format!("tiff: {e}"))?;
    Ok(())
}

fn export_gif(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let mut out = Cursor::new(Vec::new());
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut out);
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite).ok();
        if p.layers_as_frames && doc.layer_ids().len() > 1 {
            let ids = doc.layer_ids();
            for id in ids {
                let Some(l) = doc.get_layer(id) else { continue };
                if !l.visible { continue; }
                let mut img = image::RgbaImage::new(doc.w, doc.h);
                let _ = img.copy_from_slice(&l.pixels.data);
                encoder.encode_frame(image::Frame::new(img)).map_err(|e| format!("gif: {e}"))?;
            }
        } else {
            let flat = doc.flatten();
            let mut img = image::RgbaImage::new(doc.w, doc.h);
            let _ = img.copy_from_slice(&flat.data);
            encoder.encode_frame(image::Frame::new(img)).map_err(|e| format!("gif: {e}"))?;
        }
    }
    save(&p.path, out.get_ref())
}

fn export_svg(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    // Raster wrapper SVG: embed the flattened image as base64 PNG.
    let flat = doc.flatten();
    let img = image::RgbaImage::from_raw(flat.w, flat.h, flat.data.clone()).ok_or("svg buf")?;
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| format!("png: {e}"))?;
    let b64 = base64_encode(png.get_ref());
    let svg = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<image width=\"{w}\" height=\"{h}\" xlink:href=\"data:image/png;base64,{b64}\"/>\n</svg>\n",
        w = doc.w, h = doc.h
    );
    save(&p.path, svg.as_bytes())
}

fn export_ora(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let file = std::fs::File::create(&p.path).map_err(|e| format!("create: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", opts).map_err(|e| format!("zip: {e}"))?;
    zip.write_all(b"image/openraster").map_err(|e| format!("zip: {e}"))?;
    let ids = doc.layer_ids();
    let mut layers_xml = String::new();
    for (i, id) in ids.iter().enumerate().rev() {
        let l = doc.get_layer(*id).ok_or("layer missing")?;
        let name = format!("data/layer{:03}.png", ids.len() - i);
        layers_xml.push_str(&format!(
            "<layer src=\"{}\" name=\"{}\" opacity=\"{}\" visibility=\"{}\" x=\"0\" y=\"0\"/>\n",
            name, xml_escape(&l.name), (l.opacity * 255.0).round() as u32, if l.visible { "visible" } else { "hidden" }
        ));
    }
    let xml = format!(
        "<?xml version='1.0' encoding='UTF-8'?>\n<image version=\"0.0.3\" w=\"{W}\" h=\"{H}\" xres=\"72\" yres=\"72\">\n<stack>\n{layers_xml}</stack>\n</image>\n",
        W = doc.w, H = doc.h
    );
    zip.start_file("stack.xml", opts).map_err(|e| format!("zip: {e}"))?;
    zip.write_all(xml.as_bytes()).map_err(|e| format!("zip: {e}"))?;
    for (i, id) in ids.iter().enumerate().rev() {
        let l = doc.get_layer(*id).ok_or("layer missing")?;
        let name = format!("data/layer{:03}.png", ids.len() - i);
        let mut png = Cursor::new(Vec::new());
        let img = image::RgbaImage::from_raw(l.pixels.w, l.pixels.h, l.pixels.data.clone()).ok_or("ora buf")?;
        image::DynamicImage::ImageRgba8(img).write_to(&mut png, image::ImageFormat::Png).map_err(|e| format!("png: {e}"))?;
        zip.start_file(name, opts).map_err(|e| format!("zip: {e}"))?;
        zip.write_all(png.get_ref()).map_err(|e| format!("zip: {e}"))?;
    }
    let flat = doc.flatten();
    let thumb = flat.thumbnail(256, 256);
    let mut png = Cursor::new(Vec::new());
    let img = image::RgbaImage::from_raw(thumb.w, thumb.h, thumb.data.clone()).ok_or("thumb buf")?;
    image::DynamicImage::ImageRgba8(img).write_to(&mut png, image::ImageFormat::Png).map_err(|e| format!("png: {e}"))?;
    zip.start_file("Thumbnails/thumbnail.png", opts).map_err(|e| format!("zip: {e}"))?;
    zip.write_all(png.get_ref()).map_err(|e| format!("zip: {e}"))?;
    zip.finish().map_err(|e| format!("zip: {e}"))?;
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ============================ PSD ============================

fn psd_blend_id(m: BlendMode) -> &'static [u8; 4] {
    match m {
        BlendMode::Multiply => b"mul ",
        BlendMode::Screen => b"scrn",
        BlendMode::Overlay => b"ovrl",
        BlendMode::Darken => b"dark",
        BlendMode::Lighten => b"lite",
        BlendMode::ColorDodge => b"idiv",
        BlendMode::ColorBurn => b"burn",
        BlendMode::HardLight => b"hlit",
        BlendMode::SoftLight => b"sLit",
        BlendMode::Difference => b"diff",
        BlendMode::Exclusion => b"smud",
        _ => b"norm",
    }
}

/// PackBits per row; returns (counts[u16 per row], packed rows concatenated).
fn pack_bits_rows(plane: &[u8], row_len: usize) -> (Vec<u16>, Vec<u8>) {
    let mut counts = Vec::new();
    let mut data = Vec::new();
    for row in plane.chunks(row_len.max(1)) {
        let mut comp = Vec::with_capacity(row.len() + row.len() / 127 + 2);
        let mut i = 0usize;
        while i < row.len() {
            let mut run = 1usize;
            while i + run < row.len() && run < 128 && row[i + run] == row[i] {
                run += 1;
            }
            if run >= 2 {
                comp.push((257 - run as i32) as u8);
                comp.push(row[i]);
                i += run;
            } else {
                let start = i;
                let mut lit = 0usize;
                while i + lit < row.len() && lit < 128 {
                    if i + lit + 2 < row.len() && row[i + lit] == row[i + lit + 1] && row[i + lit + 1] == row[i + lit + 2] {
                        break;
                    }
                    lit += 1;
                }
                comp.push((lit - 1) as u8);
                comp.extend_from_slice(&row[start..start + lit]);
                i += lit;
            }
        }
        counts.push(comp.len() as u16);
        data.extend_from_slice(&comp);
    }
    (counts, data)
}

/// PSD writer: RGB 8-bit, layered (RLE), plus composite. Valid per PSD spec subset.
fn export_psd(doc: &mut Document, p: &ExportParams) -> Result<(), String> {
    let flat = doc.flatten();
    let (w, h) = (doc.w, doc.h);
    let ids = doc.layer_ids();

    // ---- build layer records + channel data ----
    let mut records = Vec::new(); // all layer records
    let mut all_channel_data: Vec<Vec<Vec<u8>>> = Vec::new(); // per layer: Vec<per channel bytes>
    for id in ids.iter().rev() {
        // PSD stores top-first
        let l = doc.get_layer(*id).ok_or("layer missing")?;
        let lw = l.pixels.w as usize;
        let lh = l.pixels.h as usize;
        // planar extraction: A, R, G, B
        let n = lw * lh;
        let mut pa = vec![0u8; n];
        let mut pr = vec![0u8; n];
        let mut pg = vec![0u8; n];
        let mut pb = vec![0u8; n];
        for i in 0..n {
            pa[i] = l.pixels.data[i * 4 + 3];
            pr[i] = l.pixels.data[i * 4];
            pg[i] = l.pixels.data[i * 4 + 1];
            pb[i] = l.pixels.data[i * 4 + 2];
        }
        let mut chans: Vec<(i16, Vec<u8>)> = Vec::new();
        for (cid, plane) in [(-1i16, &pa), (0, &pr), (1, &pg), (2, &pb)] {
            let (counts, data) = pack_bits_rows(plane, lw);
            let mut cd = Vec::with_capacity(2 + counts.len() * 2 + data.len());
            cd.extend_from_slice(&1u16.to_be_bytes()); // RLE compression
            for c in &counts {
                cd.extend_from_slice(&c.to_be_bytes());
            }
            cd.extend_from_slice(&data);
            chans.push((cid, cd));
        }
        // record
        let mut rec = Vec::new();
        rec.extend_from_slice(&(0i32).to_be_bytes()); // top
        rec.extend_from_slice(&(0i32).to_be_bytes()); // left
        rec.extend_from_slice(&(lh as i32).to_be_bytes()); // bottom
        rec.extend_from_slice(&(lw as i32).to_be_bytes()); // right
        rec.extend_from_slice(&4i16.to_be_bytes());
        let mut chan_lens: Vec<(i16, u32)> = Vec::new();
        for (cid, cd) in &chans {
            chan_lens.push((*cid, cd.len() as u32));
        }
        for (cid, len) in &chan_lens {
            rec.extend_from_slice(&cid.to_be_bytes());
            rec.extend_from_slice(&len.to_be_bytes());
        }
        rec.extend_from_slice(b"8BIM");
        rec.extend_from_slice(psd_blend_id(l.blend));
        rec.push(((l.opacity * 255.0).round() as u8).clamp(0, 255));
        rec.push(0u8); // clipping: base
        rec.push(if l.visible { 0 } else { 2 }); // flags: bit1 = hidden
        rec.push(0u8); // filler
        // extra data: mask (0) + ranges (0) + pascal name
        let name = &l.name;
        let nb = name.as_bytes();
        let nlen = nb.len().min(255);
        let total = 1 + nlen;
        let padded = total.div_ceil(4) * 4;
        let mut extra = Vec::new();
        extra.extend_from_slice(&0u32.to_be_bytes()); // mask data length
        extra.extend_from_slice(&0u32.to_be_bytes()); // blending ranges length
        extra.push(nlen as u8);
        extra.extend_from_slice(&nb[..nlen]);
        extra.resize(extra.len() + (padded - total), 0);
        rec.extend_from_slice(&(extra.len() as u32).to_be_bytes());
        rec.extend_from_slice(&extra);
        records.push(rec);
        all_channel_data.push(chans.into_iter().map(|(_, cd)| cd).collect());
    }

    // ---- assemble file ----
    let mut out = Vec::new();
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&[0u8; 6]);
    out.extend_from_slice(&4u16.to_be_bytes()); // channels: RGB + A
    out.extend_from_slice(&(h as u32).to_be_bytes());
    out.extend_from_slice(&(w as u32).to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes()); // RGB
    out.extend_from_slice(&0u32.to_be_bytes()); // color mode data
    out.extend_from_slice(&0u32.to_be_bytes()); // image resources

    // layer & mask info
    let mut layer_info = Vec::new();
    layer_info.extend_from_slice(&(ids.len() as i16).to_be_bytes());
    for rec in &records {
        layer_info.extend_from_slice(rec);
    }
    for cds in &all_channel_data {
        for cd in cds {
            layer_info.extend_from_slice(cd);
        }
    }
    let mut lmi = Vec::new();
    lmi.extend_from_slice(&(layer_info.len() as u32).to_be_bytes());
    lmi.extend_from_slice(&layer_info);
    lmi.extend_from_slice(&0u32.to_be_bytes()); // global layer mask info
    out.extend_from_slice(&(lmi.len() as u32).to_be_bytes());
    out.extend_from_slice(&lmi);

    // composite image data (RLE, RGB + A)
    out.extend_from_slice(&1u16.to_be_bytes());
    let n = (flat.w * flat.h) as usize;
    let mut pr = vec![0u8; n];
    let mut pg = vec![0u8; n];
    let mut pb = vec![0u8; n];
    let mut pa = vec![0u8; n];
    for i in 0..n {
        pr[i] = flat.data[i * 4];
        pg[i] = flat.data[i * 4 + 1];
        pb[i] = flat.data[i * 4 + 2];
        pa[i] = flat.data[i * 4 + 3];
    }
    // spec: one row-count table for ALL channels first, then all packed data
    let mut all_counts = Vec::new();
    let mut all_data = Vec::new();
    for plane in [&pr, &pg, &pb, &pa] {
        let (counts, data) = pack_bits_rows(plane, flat.w as usize);
        for c in &counts {
            all_counts.extend_from_slice(&c.to_be_bytes());
        }
        all_data.extend_from_slice(&data);
    }
    out.extend_from_slice(&all_counts);
    out.extend_from_slice(&all_data);
    let _ = p;
    save(&p.path, &out)
}

/// PSD reader: RGB 8-bit, layered or flattened.
fn open_psd(path: &str) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let mut r = Cursor::new(&data[..]);
    let mut sig = [0u8; 4];
    r.read_exact(&mut sig).map_err(perr)?;
    if &sig != b"8BPS" {
        return Err("not a PSD file".into());
    }
    let _version = read_u16(&mut r)?;
    r.read_exact(&mut [0u8; 6]).map_err(perr)?;
    let channels = read_u16(&mut r)? as usize;
    let h = read_u32(&mut r)?;
    let w = read_u32(&mut r)?;
    let depth = read_u16(&mut r)?;
    let mode = read_u16(&mut r)?;
    if depth != 8 || mode != 3 {
        return Err(format!("PSD: only RGB 8-bit supported (depth={depth}, mode={mode})"));
    }
    let cmlen = read_u32(&mut r)? as i64;
    r.seek_relative(cmlen).map_err(perr)?;
    let rlen = read_u32(&mut r)? as i64;
    r.seek_relative(rlen).map_err(perr)?;
    let lmi_len = read_u32(&mut r)?;
    let mut doc = Document::new(w.max(1), h.max(1), [0, 0, 0, 0], &file_stem(path));

    let mut layers_read = false;
    if lmi_len > 0 {
        let lmi_end = r.position() + lmi_len as u64;
        let li_len = read_u32(&mut r)?;
        if li_len > 0 {
            let li_end = r.position() + li_len as u64;
            let count = read_i16(&mut r)?.abs() as usize;
            let mut records: Vec<PsdLayerRecord> = Vec::new();
            for _ in 0..count {
                let top = read_i32(&mut r)?;
                let left = read_i32(&mut r)?;
                let bottom = read_i32(&mut r)?;
                let right = read_i32(&mut r)?;
                let nch = read_i16(&mut r)? as usize;
                let mut chans = Vec::new();
                for _ in 0..nch {
                    let cid = read_i16(&mut r)?;
                    let clen = read_u32(&mut r)? as usize;
                    chans.push((cid, clen));
                }
                let mut sig2 = [0u8; 4];
                r.read_exact(&mut sig2).map_err(perr)?; // "8BIM"
                let mut blend = [0u8; 4];
                r.read_exact(&mut blend).map_err(perr)?;
                let opacity = read_u8(&mut r)? as f32 / 255.0;
                let _clipping = read_u8(&mut r)?;
                let flags = read_u8(&mut r)?;
                r.read_exact(&mut [0u8; 1]).map_err(perr)?; // filler
                let extra_len = read_u32(&mut r)? as usize;
                let extra_end = r.position() + extra_len as u64;
                let mask_len = read_u32(&mut r)? as i64;
                r.seek_relative(mask_len).map_err(perr)?;
                let ranges_len = read_u32(&mut r)? as i64;
                r.seek_relative(ranges_len).map_err(perr)?;
                let nlen = read_u8(&mut r)? as usize;
                let mut nameb = vec![0u8; nlen];
                r.read_exact(&mut nameb).map_err(perr)?;
                let name = String::from_utf8_lossy(&nameb).to_string();
                r.seek_relative(extra_end as i64 - r.position() as i64).map_err(perr)?;
                records.push(PsdLayerRecord {
                    top, left, bottom, right, chans, planes: Vec::new(), name, opacity,
                    visible: (flags & 2) == 0,
                    blend: BlendMode::from_name(match &blend {
                        b"norm" => "Normal", b"mul " => "Multiply", b"scrn" => "Screen",
                        b"ovrl" => "Overlay", b"dark" => "Darken", b"lite" => "Lighten",
                        b"idiv" => "Color Dodge", b"burn" => "Color Burn", b"hlit" => "Hard Light",
                        b"sLit" => "Soft Light", b"diff" => "Difference", b"smud" => "Exclusion",
                        _ => "Normal",
                    }),
                });
            }
            // channel data
            for rec in &mut records {
                for (cid, clen) in &mut rec.chans {
                    if *clen > 0 {
                        let cd = read_exact_n(&mut r, *clen)?;
                        let comp = u16::from_be_bytes([cd[0], cd[1]]);
                        let payload = &cd[2..];
                        let lw = (rec.right - rec.left).max(0) as usize;
                        let lh = (rec.bottom - rec.top).max(0) as usize;
                        let plane = match comp {
                            0 => payload[..(lw * lh).min(payload.len())].to_vec(),
                            1 => {
                                let rows = lh;
                                let mut out = Vec::with_capacity(lw * lh);
                                let mut pos = 0usize;
                                for _ in 0..rows {
                                    if pos + 2 > payload.len() { break; }
                                    let rl = u16::from_be_bytes([payload[pos], payload[pos + 1]]) as usize;
                                    pos += 2;
                                    let end = (pos + rl).min(payload.len());
                                    out.extend_from_slice(&unpack_bits(&payload[pos..end], lw));
                                    pos = end;
                                }
                                out.resize(lw * lh, 0);
                                out
                            }
                            _ => vec![0u8; lw * lh],
                        };
                        rec.planes.push((*cid, plane));
                    }
                }
            }
            // build document layers (records are top-first; engine stores bottom-first)
            for rec in records.into_iter().rev() {
                let id = doc.alloc_id();
                let lw = (rec.right - rec.left).max(0) as u32;
                let lh = (rec.bottom - rec.top).max(0) as u32;
                let mut layer = Layer::new(id, &rec.name, w, h);
                layer.opacity = rec.opacity.clamp(0.0, 1.0);
                layer.visible = rec.visible;
                layer.blend = rec.blend;
                let mut rgba = vec![0u8; (lw * lh) as usize * 4];
                for (cid, plane) in &rec.planes {
                    match cid {
                        -1 => for i in 0..(lw * lh) as usize { rgba[i * 4 + 3] = plane[i]; },
                        0 => for i in 0..(lw * lh) as usize { rgba[i * 4] = plane[i]; },
                        1 => for i in 0..(lw * lh) as usize { rgba[i * 4 + 1] = plane[i]; },
                        2 => for i in 0..(lw * lh) as usize { rgba[i * 4 + 2] = plane[i]; },
                        _ => {}
                    }
                }
                for y in 0..lh {
                    for x in 0..lw {
                        let dx = rec.left + x as i32;
                        let dy = rec.top + y as i32;
                        if dx >= 0 && dy >= 0 && dx < w as i32 && dy < h as i32 {
                            let o = ((y * lw + x) * 4) as usize;
                            layer.pixels.set(dx, dy, [rgba[o], rgba[o + 1], rgba[o + 2], rgba[o + 3]]);
                        }
                    }
                }
                doc.root.children.push(Node::Layer(layer));
            }
            layers_read = !doc.root.children.is_empty();
            if layers_read {
                doc.active_id = doc.layer_ids()[doc.layer_ids().len() - 1];
            }
            let _ = (li_end, lmi_end);
        }
    }
    if !layers_read {
        // flattened composite
        let comp = read_u16(&mut r)?;
        let id = doc.alloc_id();
        let mut layer = Layer::new(id, "Background", w, h);
        match comp {
            0 => {
                let n = (w * h) as usize;
                let mut planes = Vec::with_capacity(channels);
                for _ in 0..channels {
                    planes.push(read_exact_n(&mut r, n)?);
                }
                fill_rgb(&mut layer, &planes, channels);
            }
            1 => {
                let rows = h as usize * channels;
                let mut counts = Vec::with_capacity(rows);
                for _ in 0..rows {
                    counts.push(read_u16(&mut r)? as usize);
                }
                let mut planes: Vec<Vec<u8>> = Vec::with_capacity(channels);
                for ch in 0..channels {
                    let mut plane = Vec::with_capacity((w * h) as usize);
                    for y in 0..h as usize {
                        let rl = counts[y * channels + ch];
                        let packed = read_exact_n(&mut r, rl)?;
                        plane.extend_from_slice(&unpack_bits(&packed, w as usize));
                    }
                    planes.push(plane);
                }
                fill_rgb(&mut layer, &planes, channels);
            }
            c => return Err(format!("PSD: unsupported compression {c}")),
        }
        doc.root.children.push(Node::Layer(layer));
        doc.active_id = id;
    }
    doc.recomposite_all();
    Ok(doc)
}

struct PsdLayerRecord {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    chans: Vec<(i16, usize)>,
    planes: Vec<(i16, Vec<u8>)>,
    name: String,
    opacity: f32,
    visible: bool,
    blend: BlendMode,
}

fn fill_rgb(layer: &mut Layer, planes: &[Vec<u8>], channels: usize) {
    let n = (layer.pixels.w * layer.pixels.h) as usize;
    let pick = |i: usize, ci: usize| -> u8 {
        planes.get(ci).and_then(|p| p.get(i).copied()).unwrap_or(255)
    };
    for i in 0..n {
        let r = pick(i, 0);
        let g = pick(i, 1);
        let b = pick(i, 2);
        let a = if channels >= 4 { pick(i, 3) } else { 255 };
        layer.pixels.data[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a]);
    }
}

fn unpack_bits(cd: &[u8], expect: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expect);
    let mut i = 0usize;
    while i < cd.len() && out.len() < expect {
        let n = cd[i] as i8;
        i += 1;
        if n >= 0 {
            let cnt = n as usize + 1;
            let end = (i + cnt).min(cd.len());
            out.extend_from_slice(&cd[i..end]);
            i = end;
        } else if n != -128 {
            let cnt = (-(n as i32)) as usize + 1;
            if i < cd.len() {
                let v = cd[i];
                i += 1;
                for _ in 0..cnt {
                    out.push(v);
                }
            }
        }
    }
    out.resize(expect, 0);
    out
}

fn perr(e: std::io::Error) -> String {
    format!("psd: {e}")
}

macro_rules! dbg_read {
    ($r:expr, $what:expr, $body:expr) => {{
        let p = $r.position();
        match $body {
            Ok(v) => v,
            Err(_) => {
                std::eprintln!("[psd-debug] {} FAILED at pos {} of {}", $what, p, $r.get_ref().len());
                return Err(format!("psd: read failed at {} ({})", p, $what));
            }
        }
    }};
}
fn read_u16(r: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut b = [0u8; 2];
    if let Err(e) = r.read_exact(&mut b) {
        std::eprintln!("[psd-debug] u16 FAILED at {} of {} ({e})", r.position(), r.get_ref().len());
        return Err(perr(e));
    }
    Ok(u16::from_be_bytes(b))
}
fn read_i16(r: &mut Cursor<&[u8]>) -> Result<i16, String> {
    let mut b = [0u8; 2];
    if let Err(e) = r.read_exact(&mut b) {
        std::eprintln!("[psd-debug] i16 FAILED at {} of {} ({e})", r.position(), r.get_ref().len());
        return Err(perr(e));
    }
    Ok(i16::from_be_bytes(b))
}
fn read_u32(r: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut b = [0u8; 4];
    if let Err(e) = r.read_exact(&mut b) {
        std::eprintln!("[psd-debug] u32 FAILED at {} of {} ({e})", r.position(), r.get_ref().len());
        return Err(perr(e));
    }
    Ok(u32::from_be_bytes(b))
}
fn read_i32(r: &mut Cursor<&[u8]>) -> Result<i32, String> {
    let mut b = [0u8; 4];
    if let Err(e) = r.read_exact(&mut b) {
        std::eprintln!("[psd-debug] i32 FAILED at {} of {} ({e})", r.position(), r.get_ref().len());
        return Err(perr(e));
    }
    Ok(i32::from_be_bytes(b))
}
fn read_u8(r: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut b = [0u8; 1];
    if let Err(e) = r.read_exact(&mut b) {
        std::eprintln!("[psd-debug] u8 FAILED at {} of {} ({e})", r.position(), r.get_ref().len());
        return Err(perr(e));
    }
    Ok(b[0])
}
fn read_exact_n(r: &mut Cursor<&[u8]>, n: usize) -> Result<Vec<u8>, String> {
    let mut v = vec![0u8; n];
    r.read_exact(&mut v).map_err(perr)?;
    Ok(v)
}
