const CCHDEVICENAME: usize = 32;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};

#[derive(Clone)]
pub struct DisplayInfo {
    pub handle: HMONITOR,
    pub display_name: String,
}

impl DisplayInfo {
    pub fn new(monitor_handle: HMONITOR) -> Self {
        #[repr(C)]
        struct MonitorInfoExW {
            _base: MONITORINFO,
            sz_device: [u16; CCHDEVICENAME],
        }

        let mut info = MonitorInfoExW {
            _base: MONITORINFO {
                cbSize: std::mem::size_of::<MonitorInfoExW>() as u32,
                rcMonitor: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                rcWork: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                dwFlags: 0,
            },
            sz_device: [0u16; CCHDEVICENAME],
        };

        unsafe {
            let result = GetMonitorInfoW(monitor_handle, &mut info as *mut _ as *mut _);
            if result.as_bool() == false {
                panic!("GetMonitorInfoW failed!");
            }
        }

        let display_name = String::from_utf16_lossy(&info.sz_device)
            .trim_matches(char::from(0))
            .to_string();

        Self {
            handle: monitor_handle,
            display_name,
        }
    }
}

pub fn enumerate_displays() -> Box<Vec<DisplayInfo>> {
    unsafe {
        let displays = Box::into_raw(Box::new(Vec::<DisplayInfo>::new()));
        EnumDisplayMonitors(
            HDC(0),
            std::ptr::null_mut(),
            Some(enum_monitor),
            LPARAM(displays as isize),
        );
        Box::from_raw(displays)
    }
}

extern "system" fn enum_monitor(monitor: HMONITOR, _: HDC, _: *mut RECT, state: LPARAM) -> BOOL {
    unsafe {
        let state = Box::leak(Box::from_raw(state.0 as *mut Vec<DisplayInfo>));
        let display_info = DisplayInfo::new(monitor);
        state.push(display_info);
    }
    true.into()
}
