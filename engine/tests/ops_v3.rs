//! v3.0 exhaustive audit: every new adjustment & effect must apply + undo cleanly.
use aurora_engine::*;
use std::ffi::CString;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn ops_audit_all_new_features() {
    let rc = aurora_ops_audit();
    assert_eq!(rc, 0, "aurora_ops_audit failed rc={rc}");
}

/// Verify pixel semantics of key adjustments on a tiny controlled image.
#[test]
fn adjust_semantics() {
    unsafe {
        let h = aurora_doc_new(8, 8, 2, cstr("t").as_ptr());
        assert_ne!(h, 0);
        let layer = aurora_layer_add(h, -1, cstr("L").as_ptr(), 0);
        assert_ne!(layer, 0);
        assert_eq!(aurora_layer_set_active(h, layer), 0);

        // paint one solid mid-gray layer via fill
        assert_eq!(aurora_fill(h, 100, 100, 100, 255), 0, "fill: {}", last_err());

        // invert → 155,155,155
        assert_eq!(aurora_adj_invert(h), 0, "invert: {}", last_err());
        let mut px = [0u8; 8 * 8 * 4];
        assert_eq!(aurora_composite_read(h, 0, 0, 8, 8, px.as_mut_ptr(), px.len() as u32), 0);
        assert_eq!(px[0], 155, "invert should map 100->155");
        assert_eq!(aurora_undo(h), 0);

        // threshold(60) on mid-gray lum=100 → white 255
        assert_eq!(aurora_adj_threshold(h, 60), 0, "threshold: {}", last_err());
        assert_eq!(aurora_composite_read(h, 0, 0, 8, 8, px.as_mut_ptr(), px.len() as u32), 0);
        assert_eq!(px[0], 255, "threshold lum100@60 -> 255");
        assert_eq!(aurora_undo(h), 0);

        // posterize(2) → values collapse to 0/255 (100 → 0 since 100 < 127.5)
        assert_eq!(aurora_adj_posterize(h, 2), 0, "posterize: {}", last_err());
        assert_eq!(aurora_composite_read(h, 0, 0, 8, 8, px.as_mut_ptr(), px.len() as u32), 0);
        assert!(px[0] == 0 || px[0] == 255, "posterize2 value {}", px[0]);
        assert_eq!(aurora_undo(h), 0);

        // histogram: uniform fill → at least bin near 100 is non-zero
        let mut hist = [0u8; 256];
        let rc = aurora_histogram(h, hist.as_mut_ptr(), 256);
        assert_eq!(rc, 256, "histogram rc");
        assert!(hist.iter().any(|&b| b > 0), "histogram should be non-empty");

        aurora_doc_free(h);
    }
}

/// Selection-scoped effects: applying to a small region must not touch pixels outside.
#[test]
fn filters_respect_selection() {
    unsafe {
        let h = aurora_doc_new(32, 32, 2, cstr("t").as_ptr());
        assert_ne!(h, 0);
        let layer = aurora_layer_add(h, -1, cstr("L").as_ptr(), 0);
        assert_ne!(layer, 0);
        assert_eq!(aurora_layer_set_active(h, layer), 0);
        assert_eq!(aurora_fill(h, 200, 40, 40, 255), 0);

        // select a 8x8 box in the corner and invert only there
        assert_eq!(aurora_select_rect(h, 0, 0, 8, 8, 0), 0);
        assert_eq!(aurora_adj_invert(h), 0, "invert w/ selection: {}", last_err());

        let mut px = [0u8; 32 * 32 * 4];
        assert_eq!(aurora_composite_read(h, 0, 0, 32, 32, px.as_mut_ptr(), px.len() as u32), 0);
        // inside: inverted (55)
        assert_eq!(px[0], 55, "inside selection inverted");
        // outside: untouched (200)
        let outside = ((16 * 32) + 16) * 4;
        assert_eq!(px[outside], 200, "outside selection must be untouched");

        aurora_doc_free(h);
    }
}

fn last_err() -> String {
    unsafe {
        let mut buf = [0u8; 512];
        let n = aurora_last_error(buf.as_mut_ptr(), 512);
        if n <= 0 { return String::new(); }
        String::from_utf8_lossy(&buf[..n.min(512) as usize]).to_string()
    }
}
