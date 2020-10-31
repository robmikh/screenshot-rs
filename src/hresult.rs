pub trait AsHresult {
    fn as_hresult(&self) -> winrt::Result<()>;
}

impl AsHresult for u32 {
    fn as_hresult(&self) -> winrt::Result<()> {
        winrt::ErrorCode(*self).ok()
    }
}

impl AsHresult for i32 {
    fn as_hresult(&self) -> winrt::Result<()> {
        winrt::ErrorCode(*self as u32).ok()
    }
}
