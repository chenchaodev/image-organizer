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

  // 延迟请求缩略图：快速滚动时单元格短暂挂载即卸载，立即请求会让大量
  // 已滚过的单元格请求堆积在 Rust 解码队列（4 并发），饿死当前可见格
  // （实测大库尾部缩略图长时间不显示）。延迟 200ms 过滤快速滚过的单元格。
  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    const timer = setTimeout(() => {
      invoke<string | null>("get_thumbnail_path", { id: item.id })
        .then((path) => {
          if (!cancelled && path) setSrc(convertFileSrc(path));
        })
        .catch(() => {
          /* 缩略图生成失败按占位块处理 */
        });
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(timer);
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
