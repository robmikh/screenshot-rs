use crate::window_info::WindowInfo;
use bindings::windows::win32::menu_rc::{EnumWindows, GetWindowLongW, GetAncestor, IsWindowVisible, GetShellWindow};
use bindings::windows::win32::base::{WS_EX_TOOLWINDOW, GWL_STYLE, WS_DISABLED, GWL_EXSTYLE, GA_ROOT};
use bindings::windows::win32::dwm::{DWMWINDOWATTRIBUTE, DwmGetWindowAttribute};

pub fn enumerate_capturable_windows() -> Box<Vec<WindowInfo>> {
    unsafe {
        let windows = Box::into_raw(Box::new(Vec::<WindowInfo>::new()));
        EnumWindows(Some(enum_window), windows as isize);
        Box::from_raw(windows)
    }
}

extern "system" fn enum_window(window: isize, state: isize) -> i32 {
    unsafe {
        let state = Box::leak(Box::from_raw(state as *mut Vec<WindowInfo>));

        let window_info = WindowInfo::new(window);
        if window_info.is_capturable_window() {
            state.push(window_info);
        }
    }
    1
}

pub trait CaptureWindowCandidate {
    fn is_capturable_window(&self) -> bool;
}

impl CaptureWindowCandidate for WindowInfo {
    fn is_capturable_window(&self) -> bool {
        unsafe {
            if self.title.is_empty()
                || self.handle == GetShellWindow()
                || IsWindowVisible(self.handle) == 0
                || GetAncestor(self.handle, GA_ROOT as u32) != self.handle
            {
                return false;
            }

            let style = GetWindowLongW(self.handle, GWL_STYLE);
            if style & (WS_DISABLED as i32) == 1 {
                return false;
            }

            // No tooltips
            let ex_style = GetWindowLongW(self.handle, GWL_EXSTYLE);
            if ex_style & WS_EX_TOOLWINDOW == 1 {
                return false;
            }

            // Check to see if the self is cloaked if it's a UWP
            if self.class_name == "Windows.UI.Core.CoreWindow"
                || self.class_name == "ApplicationFrameWindow"
            {
                let mut cloaked: u32 = 0;
                if DwmGetWindowAttribute(
                    self.handle,
                    std::mem::transmute::<_ , u32>(DWMWINDOWATTRIBUTE::DWMWA_CLOAKED),
                    &mut cloaked as *mut _ as *mut _,
                    std::mem::size_of::<u32>() as u32,
                ) == 0
                    && cloaked == /* DWM_CLOAKED_SHELL is missing... */ 0x0000002
                {
                    return false;
                }
            }

            // Unfortunate work-around. Not sure how to avoid this.
            if is_known_blocked_window(self) {
                return false;
            }
        }
        true
    }
}

fn is_known_blocked_window(window_info: &WindowInfo) -> bool {
    // Task View
    window_info.matches_title_and_class_name("Task View", "Windows.UI.Core.CoreWindow") ||
    // XAML Islands
    window_info.matches_title_and_class_name("DesktopWindowXamlSource", "Windows.UI.Core.CoreWindow") ||
    // XAML Popups
    window_info.matches_title_and_class_name("PopupHost", "Xaml_WindowedPopupClass")
}
