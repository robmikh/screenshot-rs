use windows::core::Result;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};

#[derive(Clone)]
pub struct DisplayInfo {
    pub handle: HMONITOR,
}

impl DisplayInfo {
    pub fn new(monitor_handle: HMONITOR) -> Result<Self> {
        let mut info = MONITORINFO::default();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

        unsafe {
            GetMonitorInfoW(monitor_handle, &mut info as *mut _ as *mut _).ok()?;
        }

        Ok(Self {
            handle: monitor_handle,
        })
    }
}

pub fn enumerate_displays() -> Result<Vec<DisplayInfo>> {
    unsafe {
        let displays = Box::into_raw(Box::default());
        EnumDisplayMonitors(None, None, Some(enum_monitor), LPARAM(displays as isize)).ok()?;
        Ok(*Box::from_raw(displays))
    }
}

extern "system" fn enum_monitor(monitor: HMONITOR, _: HDC, _: *mut RECT, state: LPARAM) -> BOOL {
    unsafe {
        let state = Box::leak(Box::from_raw(state.0 as *mut Vec<DisplayInfo>));
        let display_info = DisplayInfo::new(monitor).unwrap();
        state.push(display_info);
    }
    true.into()
}
