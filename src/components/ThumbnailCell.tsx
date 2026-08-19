import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { ImageItem } from "../types";

interface ThumbnailCellProps {
  item: ImageItem;
  selected: boolean;
  onSelect: (id: number) => void;
}

export default function ThumbnailCell({ item, selected, onSelect }: ThumbnailCellProps) {
  const [src, setSrc] = useState<string | null>(null);

  // 可见单元格挂载时获取缩略图路径；null 或失败一律显示灰色占位块。
  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    invoke<string | null>("get_thumbnail_path", { id: item.id })
      .then((path) => {
        if (!cancelled && path) setSrc(convertFileSrc(path));
      })
      .catch(() => {
        /* 缩略图生成失败按占位块处理 */
      });
    return () => {
      cancelled = true;
    };
  }, [item.id]);

  return (
    <button
      type="button"
      className={`grid-cell${selected ? " selected" : ""}`}
      title={item.path}
      onClick={() => onSelect(item.id)}
    >
      {src ? <img src={src} alt="" loading="lazy" draggable={false} /> : <span className="cell-placeholder" />}
    </button>
  );
}
