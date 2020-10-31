pub const DWMWA_CLOAKED: u32 = 14;
pub const DWM_CLOAKED_SHELL: u32 = 2;

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmGetWindowAttribute(
        hWnd: isize,
        attribute: u32,
        attribute_value: *mut ::std::ffi::c_void,
        attribute_size: u32,
    ) -> i32;
}
