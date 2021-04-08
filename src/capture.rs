use crate::window_info::WindowInfo;
use bindings::Windows::Win32::Dwm::{DwmGetWindowAttribute, DWMWINDOWATTRIBUTE};
use bindings::Windows::Win32::SystemServices::{GetConsoleWindow, BOOL};
use bindings::Windows::Win32::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetAncestor_gaFlags, GetShellWindow, GetWindowLongW, IsWindowVisible,
    HWND, LPARAM, WINDOW_EX_STYLE, WINDOW_LONG_PTR_INDEX, WINDOW_STYLE,
};

struct WindowEnumerationState {
    windows: Vec<WindowInfo>,
    console_window: Option<HWND>,
}

pub fn enumerate_capturable_windows() -> Vec<WindowInfo> {
    unsafe {
        // TODO: This works for Command Prompt but not Terminal
        let console_window = {
            let window_handle = GetConsoleWindow();
            if window_handle.0 == 0 {
                None
            } else {
                Some(window_handle)
            }
        };
        let state = Box::into_raw(Box::new(WindowEnumerationState {
            windows: Vec::new(),
            console_window,
        }));
        EnumWindows(Some(enum_window), LPARAM(state as isize));
        let state = Box::from_raw(state);
        state.windows
    }
}

extern "system" fn enum_window(window: HWND, state: LPARAM) -> BOOL {
    unsafe {
        let state = Box::leak(Box::from_raw(state.0 as *mut WindowEnumerationState));

        if let Some(console_window) = &state.console_window {
            if window == *console_window {
                return true.into();
            }
        }

        let window_info = WindowInfo::new(window);
        if window_info.is_capturable_window() {
            state.windows.push(window_info);
        }
    }
    true.into()
}

pub trait CaptureWindowCandidate {
    fn is_capturable_window(&self) -> bool;
}

impl CaptureWindowCandidate for WindowInfo {
    fn is_capturable_window(&self) -> bool {
        unsafe {
            if self.title.is_empty()
                || self.handle == GetShellWindow()
                || IsWindowVisible(self.handle).as_bool() == false
                || GetAncestor(self.handle, GetAncestor_gaFlags::GA_ROOT) != self.handle
            {
                return false;
            }

            let style = GetWindowLongW(self.handle, WINDOW_LONG_PTR_INDEX::GWL_STYLE);
            if style & (WINDOW_STYLE::WS_DISABLED.0 as i32) == 1 {
                return false;
            }

            // No tooltips
            let ex_style = GetWindowLongW(self.handle, WINDOW_LONG_PTR_INDEX::GWL_EXSTYLE);
            if ex_style & (WINDOW_EX_STYLE::WS_EX_TOOLWINDOW.0 as i32) == 1 {
                return false;
            }

            // Check to see if the self is cloaked if it's a UWP
            if self.class_name == "Windows.UI.Core.CoreWindow"
                || self.class_name == "ApplicationFrameWindow"
            {
                let mut cloaked: u32 = 0;
                if DwmGetWindowAttribute(
                    self.handle,
                    std::mem::transmute::<_, u32>(DWMWINDOWATTRIBUTE::DWMWA_CLOAKED),
                    &mut cloaked as *mut _ as *mut _,
                    std::mem::size_of::<u32>() as u32,
                )
                .is_ok()
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
