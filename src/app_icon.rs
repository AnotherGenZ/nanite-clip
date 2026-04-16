use std::sync::LazyLock;

use iced::window;
use image::GenericImageView;
#[cfg(target_os = "linux")]
use ksni::Icon as KsniIcon;
use tray_icon::Icon as TrayIcon;

const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/NaniteClips.png");

static WINDOW_ICON: LazyLock<Option<window::Icon>> = LazyLock::new(|| {
    let image =
        image::load_from_memory_with_format(APP_ICON_BYTES, image::ImageFormat::Png).ok()?;
    let (width, height) = image.dimensions();
    let rgba = image.into_rgba8().into_vec();

    window::icon::from_rgba(rgba, width, height).ok()
});

#[cfg(target_os = "linux")]
static TRAY_ICON: LazyLock<Vec<KsniIcon>> = LazyLock::new(|| {
    let Some(image) =
        image::load_from_memory_with_format(APP_ICON_BYTES, image::ImageFormat::Png).ok()
    else {
        return Vec::new();
    };

    let (width, height) = image.dimensions();
    let mut data = image.into_rgba8().into_vec();

    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }

    vec![KsniIcon {
        width: width as i32,
        height: height as i32,
        data,
    }]
});

pub fn window_icon() -> Option<window::Icon> {
    WINDOW_ICON.clone()
}

#[cfg(target_os = "linux")]
pub fn tray_icon() -> Vec<KsniIcon> {
    TRAY_ICON.clone()
}

pub fn tray_icon_cross_platform() -> Option<TrayIcon> {
    let image =
        image::load_from_memory_with_format(APP_ICON_BYTES, image::ImageFormat::Png).ok()?;
    let (width, height) = image.dimensions();
    TrayIcon::from_rgba(image.into_rgba8().into_vec(), width, height).ok()
}
