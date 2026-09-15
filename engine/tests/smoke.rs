use aurora_engine::*;
use std::ffi::CString;

#[test]
fn engine_end_to_end() {
    let mut buf = [0u8; 256];
    unsafe {
        aurora_version(buf.as_mut_ptr(), 256);
        let ver = std::ffi::CStr::from_ptr(buf.as_ptr() as *const _).to_string_lossy().to_string();
        println!("engine: {ver}");
        assert!(ver.contains("Aurora Engine"));

        assert_eq!(aurora_selftest(), 0, "selftest failed: {}", {
            aurora_last_error(buf.as_mut_ptr(), 256);
            std::ffi::CStr::from_ptr(buf.as_ptr() as *const _).to_string_lossy().to_string()
        });
    }
    let _ = CString::new("").unwrap();
}
