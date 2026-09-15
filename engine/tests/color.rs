use aurora_engine::*;
use std::ffi::CString;

#[test]
fn brush_color_check() {
    let mut buf = [0u8; 256];
    unsafe {
        let h = aurora_doc_new(100, 100, 2, c"t".as_ptr());
        assert!(h != 0);
        let layer = aurora_layer_add(h, -1, c"L".as_ptr(), 0);
        let bp = BrushFFI {
            size: 20.0, hardness: 0.9, flow: 1.0, opacity: 1.0, spacing: 0.1,
            eraser: 0, r: 255, g: 154, b: 60, a: 255,
            size_pressure: 0, opacity_pressure: 0, pencil: 0,
        };
        assert_eq!(aurora_brush_begin(h, layer, &bp, 50.0, 50.0, 1.0), 0, "{}", {
            aurora_last_error(buf.as_mut_ptr(), 256);
            std::ffi::CStr::from_ptr(buf.as_ptr() as *const _).to_string_lossy().to_string()
        });
        aurora_brush_move(h, 55.0, 52.0, 1.0);
        aurora_brush_end(h);
        // read composite pixel at stroke center
        let mut px = [0u8; 4];
        assert_eq!(aurora_composite_read(h, 50, 50, 1, 1, px.as_mut_ptr(), 4), 0);
        println!("pixel at stroke: {:?}", px);
        // expect orange-ish: r > 200, g mid, b low
        assert!(px[0] > 180, "red channel {} too low", px[0]);
        assert!(px[2] < 150, "blue channel {} too high", px[2]);
        aurora_doc_free(h);
    }
    let _ = CString::new("").unwrap();
}
