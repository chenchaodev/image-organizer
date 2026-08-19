//! 图片元数据探测：尺寸/格式 + EXIF（纯逻辑层，不依赖 Tauri）。
//!
//! 为什么独立成模块：扫描器（scanner.rs）在入库前需要尺寸/格式，
//! 详情查询（images.rs）需要 EXIF，两者共用本模块避免重复实现；
//! 且探测逻辑可脱离扫描流程单独测试。

use std::collections::HashMap;
use std::path::Path;

/// 尺寸/格式探测结果。
pub struct ImageProbe {
    pub width: u32,
    pub height: u32,
    /// 格式名小写（如 "png"/"jpeg"），与 image crate 的 ImageFormat 变体对应。
    pub format: String,
}

/// 读取图片尺寸与格式。
///
/// 为什么用 image_dimensions 而非整图解码：它只读文件头（尺寸/格式
/// 信息都在头部），对超大图也只需毫秒级 I/O，扫描大量文件时开销可控。
/// 失败返回可读错误，由调用方决定降级策略（扫描器降级为尺寸留空入库）。
pub fn probe_image(path: &Path) -> Result<ImageProbe, String> {
    let (width, height) = image::image_dimensions(path)
        .map_err(|e| format!("读取图片尺寸失败：{e}"))?;
    let format = image::ImageFormat::from_path(path)
        .map_err(|e| format!("识别图片格式失败：{e}"))?;
    Ok(ImageProbe {
        width,
        height,
        format: format!("{format:?}").to_lowercase(),
    })
}

/// 提取 EXIF 可读键值；无 EXIF 或解析失败返回空 map（失败降级，不报错）。
///
/// 为什么失败降级而非报错：EXIF 是可选元数据，绝大多数图片没有 EXIF，
/// 把「没有 EXIF」当错误会让详情查询处处处理异常分支；空 map 语义更干净。
///
/// 为什么白名单而非全量导出：EXIF 标准含数百个标签，全量导出会把大量
/// 无意义/重复的键（如各 IFD 指针）塞进前端；白名单只保留用户关心的
/// 拍摄信息，键名固定可预测（前端可直接按名取用）。
pub fn read_exif(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return map,
    };
    let mut reader = std::io::BufReader::new(&file);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(_) => return map,
    };
    for field in exif.fields() {
        let Some(key) = readable_tag_name(field.tag) else {
            continue;
        };
        // 同一标签可能出现在多个 IFD（如 DateTimeOriginal 在 IFD0 与 ExifIFD），
        // 取第一个出现的值即可，避免重复键覆盖。
        if map.contains_key(key) {
            continue;
        }
        // with_unit 让 Rational 值带上单位（如焦距 "50 mm"、光圈 "f/2.8"）；
        // 再统一清理 Ascii 值可能带有的 \0 结尾。
        let value = field
            .display_value()
            .with_unit(&exif)
            .to_string()
            .trim_matches('\0')
            .trim()
            .to_string();
        map.insert(key.to_string(), value);
    }
    map
}

/// 白名单：标签 → 可读键名。只保留拍摄信息相关标签；
/// 未列出的标签（IFD 指针、缩略图偏移等内部结构）不导出。
fn readable_tag_name(tag: exif::Tag) -> Option<&'static str> {
    use exif::Tag;
    let name = match tag {
        // 拍摄时间
        Tag::DateTimeOriginal => "DateTimeOriginal",
        Tag::DateTime => "DateTime",
        Tag::DateTimeDigitized => "DateTimeDigitized",
        Tag::OffsetTime => "OffsetTime",
        Tag::OffsetTimeOriginal => "OffsetTimeOriginal",
        Tag::OffsetTimeDigitized => "OffsetTimeDigitized",
        // 相机与镜头
        Tag::Make => "Make",
        Tag::Model => "Model",
        Tag::LensMake => "LensMake",
        Tag::LensModel => "LensModel",
        Tag::LensSpecification => "LensSpecification",
        Tag::Software => "Software",
        Tag::Artist => "Artist",
        Tag::Copyright => "Copyright",
        Tag::ImageDescription => "ImageDescription",
        Tag::CameraOwnerName => "CameraOwnerName",
        Tag::BodySerialNumber => "BodySerialNumber",
        Tag::LensSerialNumber => "LensSerialNumber",
        // 曝光参数
        Tag::PhotographicSensitivity => "ISO",
        Tag::FocalLength => "FocalLength",
        Tag::FocalLengthIn35mmFilm => "FocalLengthIn35mmFilm",
        Tag::ExposureTime => "ExposureTime",
        Tag::FNumber => "FNumber",
        Tag::ExposureBiasValue => "ExposureBiasValue",
        Tag::ExposureProgram => "ExposureProgram",
        Tag::ExposureMode => "ExposureMode",
        Tag::MeteringMode => "MeteringMode",
        Tag::WhiteBalance => "WhiteBalance",
        Tag::Flash => "Flash",
        Tag::ShutterSpeedValue => "ShutterSpeedValue",
        Tag::ApertureValue => "ApertureValue",
        Tag::BrightnessValue => "BrightnessValue",
        Tag::MaxApertureValue => "MaxApertureValue",
        Tag::SubjectDistance => "SubjectDistance",
        Tag::DigitalZoomRatio => "DigitalZoomRatio",
        Tag::SceneCaptureType => "SceneCaptureType",
        Tag::Contrast => "Contrast",
        Tag::Saturation => "Saturation",
        Tag::Sharpness => "Sharpness",
        Tag::GainControl => "GainControl",
        Tag::SubjectDistanceRange => "SubjectDistanceRange",
        Tag::SensingMethod => "SensingMethod",
        Tag::LightSource => "LightSource",
        Tag::CustomRendered => "CustomRendered",
        Tag::FileSource => "FileSource",
        Tag::SceneType => "SceneType",
        // 图像属性
        Tag::Orientation => "Orientation",
        Tag::ColorSpace => "ColorSpace",
        Tag::ExifVersion => "ExifVersion",
        Tag::FlashpixVersion => "FlashpixVersion",
        Tag::PixelXDimension => "PixelXDimension",
        Tag::PixelYDimension => "PixelYDimension",
        Tag::CompressedBitsPerPixel => "CompressedBitsPerPixel",
        Tag::ComponentsConfiguration => "ComponentsConfiguration",
        Tag::ImageUniqueID => "ImageUniqueID",
        Tag::UserComment => "UserComment",
        // GPS
        Tag::GPSLatitude => "GPSLatitude",
        Tag::GPSLatitudeRef => "GPSLatitudeRef",
        Tag::GPSLongitude => "GPSLongitude",
        Tag::GPSLongitudeRef => "GPSLongitudeRef",
        Tag::GPSAltitude => "GPSAltitude",
        Tag::GPSAltitudeRef => "GPSAltitudeRef",
        Tag::GPSTimeStamp => "GPSTimeStamp",
        Tag::GPSDateStamp => "GPSDateStamp",
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 临时目录：唯一子目录避免并行测试互相干扰，测试后清理。
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("io_meta_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// image crate 生成的图不带 EXIF：断言尺寸/格式探测正确 + EXIF 空 map。
    #[test]
    fn probe_dimensions_and_format() {
        let dir = temp_dir("probe");
        let png = dir.join("t.png");
        let img = image::RgbaImage::from_pixel(16, 9, image::Rgba([1u8, 2, 3, 255]));
        img.save(&png).unwrap();

        let probe = probe_image(&png).unwrap();
        assert_eq!((probe.width, probe.height), (16, 9));
        assert_eq!(probe.format, "png");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 无 EXIF 的图返回空 map；不存在的文件也返回空 map（失败降级不报错）。
    #[test]
    fn no_exif_returns_empty_map() {
        let dir = temp_dir("no_exif");
        let png = dir.join("t.png");
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([0u8, 0, 0, 255]));
        img.save(&png).unwrap();

        assert!(read_exif(&png).is_empty());
        assert!(read_exif(&dir.join("nope.png")).is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}