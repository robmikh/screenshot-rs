use windows::{
    core::{Result, GUID},
    Win32::{
        Graphics::Imaging::{
            CLSID_WICImagingFactory, IWICBitmapDecoder, IWICImagingFactory,
            WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        },
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    },
};

pub fn create_wic_factory() -> Result<IWICImagingFactory> {
    let wic_factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };
    Ok(wic_factory)
}

pub struct WICImage {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bytes: Vec<u8>,
}

pub fn load_image_from_decoder(
    wic_factory: &IWICImagingFactory,
    decoder: &IWICBitmapDecoder,
    pixel_format: &GUID,
    bytes_per_pixel: u32,
) -> Result<WICImage> {
    // Get our image from the decoder and make sure it's in the FP16 format we need
    let frame = unsafe { decoder.GetFrame(0)? };
    let converter = unsafe { wic_factory.CreateFormatConverter()? };
    let (width, height) = unsafe {
        converter.Initialize(
            &frame,
            pixel_format,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        )?;
        let mut width = 0;
        let mut height = 0;
        converter.GetSize(&mut width, &mut height)?;
        (width, height)
    };
    let stride = bytes_per_pixel * width;
    let buffer_size = stride * height;
    let mut bytes = vec![0u8; buffer_size as usize];
    unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut bytes)? };

    Ok(WICImage {
        width,
        height,
        stride,
        bytes,
    })
}
