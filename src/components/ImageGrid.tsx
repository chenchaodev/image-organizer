import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ImageItem } from "../types";
import ThumbnailCell from "./ThumbnailCell";

const CELL_SIZE = 160;
const GAP = 12;
const GRID_PADDING = 16; // 需与 .grid-row 的 padding 一致
const OVERSCAN = 6;

interface ImageGridProps {
  items: ImageItem[];
  total: number;
  loading: boolean;
  selectedId: number | null;
  onLoadMore: () => void;
  onSelect: (id: number) => void;
}

export default function ImageGrid({
  items,
  total,
  loading,
  selectedId,
  onLoadMore,
  onSelect,
}: ImageGridProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [columnCount, setColumnCount] = useState(4);

  // 容器宽度变化时自适应列数
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const compute = () => {
      const available = el.clientWidth - GRID_PADDING * 2;
      const cols = Math.max(1, Math.floor((available + GAP) / (CELL_SIZE + GAP)));
      setColumnCount(cols);
    };
    compute();
    const ro = new ResizeObserver(compute);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const rowCount = Math.ceil(items.length / columnCount);
  const hasMore = items.length < total;

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => CELL_SIZE + GAP,
    overscan: OVERSCAN,
  });

  const virtualRows = virtualizer.getVirtualItems();
  const lastRow = virtualRows[virtualRows.length - 1];

  // 接近底部时触发下一页加载
  useEffect(() => {
    if (lastRow && lastRow.index >= rowCount - 3 && hasMore && !loading) {
      onLoadMore();
    }
  }, [lastRow?.index, rowCount, hasMore, loading, onLoadMore]);

  return (
    <div ref={scrollRef} className="grid-scroll">
      <div className="grid-inner" style={{ height: virtualizer.getTotalSize() + GRID_PADDING * 2 }}>
        {virtualRows.map((row) => {
          const start = row.index * columnCount;
          const rowItems = items.slice(start, start + columnCount);
          return (
            <div
              key={row.key}
              className="grid-row"
              style={{ transform: `translateY(${row.start}px)` }}
            >
              {rowItems.map((item) => (
                <ThumbnailCell
                  key={item.id}
                  item={item}
                  selected={item.id === selectedId}
                  onSelect={onSelect}
                />
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
