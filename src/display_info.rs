const CCHDEVICENAME: usize = 32;
use bindings::windows::win32::backup::RECT;
use bindings::windows::win32::base::BOOL;
use bindings::windows::win32::base::LPARAM;
use bindings::windows::win32::gdi::HDC;
use bindings::windows::win32::menu_rc::{EnumDisplayMonitors, GetMonitorInfoW, MONITORINFO};

#[derive(Clone)]
pub struct DisplayInfo {
    pub handle: isize,
    pub display_name: String,
}

impl DisplayInfo {
    pub fn new(monitor_handle: isize) -> Self {
        #[repr(C)]
        struct MonitorInfoExW {
            _base: MONITORINFO,
            sz_device: [u16; CCHDEVICENAME],
        }

        let mut info = MonitorInfoExW {
            _base: MONITORINFO {
                cb_size: std::mem::size_of::<MonitorInfoExW>() as u32,
                rc_monitor: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                rc_work: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                dw_flags: 0,
            },
            sz_device: [0u16; CCHDEVICENAME],
        };

        unsafe {
            let result = GetMonitorInfoW(monitor_handle, &mut info as *mut _ as *mut _);
            if result == false.into() {
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

extern "system" fn enum_monitor(monitor: isize, _: HDC, _: *mut RECT, state: LPARAM) -> BOOL {
    unsafe {
        let state = Box::leak(Box::from_raw(state.0 as *mut Vec<DisplayInfo>));

        let display_info = DisplayInfo::new(monitor);
        state.push(display_info);
    }
    true.into()
}
