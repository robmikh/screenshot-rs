fn main() {
    winrt::build!(
        types
            windows::ui::Colors
            windows::graphics::capture::{
                Direct3D11CaptureFramePool,
                Direct3D11CaptureFrame,
                GraphicsCaptureSession,
                GraphicsCaptureItem,
            }
            win_rt_interop_tools::{
                Direct3D11Device,
                Direct3D11DeviceContext,
            }
    );
}
