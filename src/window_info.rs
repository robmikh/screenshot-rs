use windows::Win32::Foundation::{HWND, PWSTR};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowTextW};

#[derive(Clone)]
pub struct WindowInfo {
    pub handle: HWND,
    pub title: String,
    pub class_name: String,
}

impl WindowInfo {
    // TODO: Return result?
    pub fn new(window_handle: HWND) -> Self {
        unsafe {
            let mut title = [0u16; 512];
            GetWindowTextW(window_handle, PWSTR(title.as_mut_ptr()), title.len() as i32);
            let mut title = String::from_utf16_lossy(&title);
            truncate_to_first_null_char(&mut title);

            let mut class_name = [0u16; 512];
            GetClassNameW(
                window_handle,
                PWSTR(class_name.as_mut_ptr()),
                class_name.len() as i32,
            );
            let mut class_name = String::from_utf16_lossy(&class_name);
            truncate_to_first_null_char(&mut class_name);

            Self {
                handle: window_handle,
                title,
                class_name,
            }
        }
    }

    pub fn matches_title_and_class_name(&self, title: &str, class_name: &str) -> bool {
        self.title == title && self.class_name == class_name
    }
}

fn truncate_to_first_null_char(input: &mut String) {
    if let Some(index) = input.find('\0') {
        input.truncate(index);
    }
}
