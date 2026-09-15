use aurora_engine::*;
use std::ffi::CString;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// Regression tests for the ToolDemo verification failures:
/// bucket on transparent active layer, gradient on a fresh layer,
/// and read-back of the composite.
#[test]
fn bucket_gradient_composite() {
    unsafe {
        let h = aurora_doc_new(320, 240, 1, cstr("t").as_ptr());
        assert_ne!(h, 0, "doc_new");

        let layer = aurora_layer_add(h, -1, cstr("Art").as_ptr(), 0);
        assert_ne!(layer, 0, "layer_add");
        assert_eq!(aurora_layer_set_active(h, layer), 0, "set_active");

        // ---- bucket fill from transparent seed ----
        let rc = aurora_bucket(h, 4, 4, 0x2E, 0x86, 0xFF, 255, 24, 1);
        assert_eq!(rc, 0, "bucket rc: {}", last_err());

        // dump doc state (which layer is active?)
        let mut jbuf = vec![0u8; 65536];
        let n = aurora_doc_json(h, jbuf.as_mut_ptr(), jbuf.len() as u32);
        let json = String::from_utf8_lossy(&jbuf[..(n.max(0) as usize)]).to_string();
        let active_off = json.find("activeId").unwrap_or(0);
        println!("doc json around activeId: {}", &json[active_off..active_off + 40.min(json.len() - active_off)]);

        let mut px = vec![0u8; 320 * 240 * 4];
        let rc = aurora_composite_read(h, 0, 0, 320, 240, px.as_mut_ptr(), px.len() as u32);
        assert_eq!(rc, 0, "composite_read after bucket");
        let idx = (4 * 320 + 4) * 4;
        println!(
            "after bucket @4,4: {} {} {} {}",
            px[idx], px[idx + 1], px[idx + 2], px[idx + 3]
        );
        assert_eq!(
            [px[idx], px[idx + 1], px[idx + 2]],
            [0x2E, 0x86, 0xFF],
            "bucket pixel should be the fill color"
        );

        // ---- gradient on a fresh layer ----
        let grad = aurora_layer_add(h, -1, cstr("Grad").as_ptr(), 0);
        assert_ne!(grad, 0, "grad layer");
        assert_eq!(aurora_layer_set_active(h, grad), 0, "activate grad");
        let rc = aurora_gradient(h, 20.0, 20.0, 300.0, 220.0, 0, 0,
            255, 60, 120, 255, 20, 20, 40, 255, 1);
        assert_eq!(rc, 0, "gradient rc: {}", last_err());
        let rc = aurora_composite_read(h, 0, 0, 320, 240, px.as_mut_ptr(), px.len() as u32);
        assert_eq!(rc, 0, "composite_read after gradient");
        let idx = ((20 * 320) + 20) * 4;
        println!(
            "gradient start @20,20: {} {} {} {}",
            px[idx], px[idx + 1], px[idx + 2], px[idx + 3]
        );
        assert_eq!(
            [px[idx], px[idx + 1], px[idx + 2]],
            [255, 60, 120],
            "gradient start pixel should be fg color"
        );
    }
}

fn last_err() -> String {
    let mut buf = [0u8; 512];
    unsafe {
        aurora_last_error(buf.as_mut_ptr(), 512);
        std::ffi::CStr::from_ptr(buf.as_ptr() as *const _)
            .to_string_lossy()
            .to_string()
    }
}
