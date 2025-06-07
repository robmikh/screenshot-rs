use windows::{
    core::Result,
    Win32::{
        Graphics::Imaging::{CLSID_WICImagingFactory, IWICImagingFactory},
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    },
};

pub fn create_wic_factory() -> Result<IWICImagingFactory> {
    let wic_factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };
    Ok(wic_factory)
}
