import { useEffect, useMemo, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { formatDate, formatDimensions, formatFileSize } from "../lib/format";
import type { ImageDetail } from "../types";

interface DetailPanelProps {
  id: number;
  onClose: () => void;
}

// EXIF 键 → 中文标签（未知键原样显示）
const EXIF_LABELS: Record<string, string> = {
  DateTimeOriginal: "拍摄时间",
  DateTimeDigitized: "数字化时间",
  Make: "厂商",
  Model: "型号",
  LensModel: "镜头型号",
  LensMake: "镜头厂商",
  Software: "软件",
  Artist: "作者",
  Copyright: "版权",
  ISO: "ISO",
  FocalLength: "焦距",
  FocalLengthIn35mmFilm: "35mm 等效焦距",
  FNumber: "光圈",
  ApertureValue: "光圈值",
  ExposureTime: "曝光时间",
  ExposureProgram: "曝光程序",
  ExposureBiasValue: "曝光补偿",
  ShutterSpeedValue: "快门速度",
  BrightnessValue: "亮度值",
  Flash: "闪光灯",
  ImageWidth: "图像宽度",
  ImageHeight: "图像高度",
  Orientation: "方向",
  Resolution: "分辨率",
  XResolution: "水平分辨率",
  YResolution: "垂直分辨率",
  ColorSpace: "色彩空间",
  WhiteBalance: "白平衡",
  MeteringMode: "测光模式",
  ExifVersion: "EXIF 版本",
  ExifImageWidth: "EXIF 图像宽度",
  ExifImageHeight: "EXIF 图像高度",
};

// EXIF 分组顺序；未命中任何组的键归入「其他信息」
const EXIF_GROUPS: { group: string; keys: string[] }[] = [
  {
    group: "拍摄信息",
    keys: [
      "DateTimeOriginal",
      "DateTimeDigitized",
      "ExposureTime",
      "FNumber",
      "ApertureValue",
      "ISO",
      "FocalLength",
      "FocalLengthIn35mmFilm",
      "Flash",
      "ExposureProgram",
      "ExposureBiasValue",
      "ShutterSpeedValue",
      "BrightnessValue",
    ],
  },
  {
    group: "相机",
    keys: ["Make", "Model", "LensModel", "LensMake", "Software", "Artist", "Copyright"],
  },
  {
    group: "图像属性",
    keys: [
      "ImageWidth",
      "ImageHeight",
      "Orientation",
      "Resolution",
      "XResolution",
      "YResolution",
      "ColorSpace",
      "WhiteBalance",
      "MeteringMode",
      "ExifVersion",
      "ExifImageWidth",
      "ExifImageHeight",
    ],
  },
];

function groupExif(exif: Record<string, string>): Array<{ group: string; entries: Array<[string, string]> }> {
  const used = new Set<string>();
  const groups: Array<{ group: string; entries: Array<[string, string]> }> = [];
  for (const { group, keys } of EXIF_GROUPS) {
    const entries: Array<[string, string]> = [];
    for (const key of keys) {
      const value = exif[key];
      if (value !== undefined && value !== "") {
        entries.push([EXIF_LABELS[key] ?? key, value]);
        used.add(key);
      }
    }
    if (entries.length > 0) groups.push({ group, entries });
  }
  const rest = Object.entries(exif).filter(([key, value]) => !used.has(key) && value !== "");
  if (rest.length > 0) groups.push({ group: "其他信息", entries: rest });
  return groups;
}

export default function DetailPanel({ id, onClose }: DetailPanelProps) {
  const [detail, setDetail] = useState<ImageDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setError(null);
    invoke<ImageDetail>("get_image_detail", { id })
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [id]);

  const exifGroups = useMemo(() => (detail ? groupExif(detail.exif) : []), [detail]);

  return (
    <aside className="detail-panel">
      <div className="detail-header">
        <h2>图片详情</h2>
        <button type="button" className="close-button" onClick={onClose} aria-label="关闭">
          ×
        </button>
      </div>
      <div className="detail-body">
        {error && <p className="error">{error}</p>}
        {!detail && !error && <p className="hint">加载中…</p>}
        {detail && (
          <>
            <div className="detail-image">
              <img src={convertFileSrc(detail.path)} alt={detail.path} />
            </div>
            <dl className="detail-meta">
              <div className="meta-row">
                <dt>尺寸</dt>
                <dd>{formatDimensions(detail.width, detail.height)}</dd>
              </div>
              <div className="meta-row">
                <dt>格式</dt>
                <dd>{detail.format ?? "—"}</dd>
              </div>
              <div className="meta-row">
                <dt>文件大小</dt>
                <dd>{formatFileSize(detail.fileSize)}</dd>
              </div>
              <div className="meta-row">
                <dt>修改时间</dt>
                <dd>{formatDate(detail.mtime)}</dd>
              </div>
              <div className="meta-row path">
                <dt>路径</dt>
                <dd>{detail.path}</dd>
              </div>
            </dl>
            {exifGroups.length > 0 && (
              <div className="detail-exif">
                <h3>EXIF</h3>
                {exifGroups.map(({ group, entries }) => (
                  <section key={group} className="exif-group">
                    <h4>{group}</h4>
                    <dl>
                      {entries.map(([label, value]) => (
                        <div key={label} className="meta-row">
                          <dt>{label}</dt>
                          <dd>{value}</dd>
                        </div>
                      ))}
                    </dl>
                  </section>
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </aside>
  );
}
