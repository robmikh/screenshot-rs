use windows::{
    Win32::{
        Graphics::Imaging::{CLSID_WICImagingFactory, IWICImagingFactory},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    },
    core::Result,
};

pub fn create_wic_factory() -> Result<IWICImagingFactory> {
    let wic_factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };
    Ok(wic_factory)
}
