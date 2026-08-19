// 纯逻辑格式化模块：不依赖 React / Tauri，供界面与单元测试复用。

/** 文件大小 → "1.2 MB"；null 或非法值 → "—"。B 显示整数，KB 及以上保留 1 位小数。 */
export function formatFileSize(bytes: number | null): string {
  if (bytes === null || Number.isNaN(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"] as const;
  let value = bytes / 1024;
  let unit: string = units[0];
  for (const next of units.slice(1)) {
    if (value < 1024) break;
    value /= 1024;
    unit = next;
  }
  return `${value.toFixed(1)} ${unit}`;
}

/** 秒级时间戳 → "YYYY-MM-DD HH:mm"；null 或非法值 → "—"。 */
export function formatDate(epochSecs: number | null): string {
  if (epochSecs === null || Number.isNaN(epochSecs) || epochSecs <= 0) return "—";
  const d = new Date(epochSecs * 1000);
  const pad = (n: number): string => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 宽 × 高；任一为 null 或 ≤0 → "—"。 */
export function formatDimensions(w: number | null, h: number | null): string {
  if (w === null || h === null || w <= 0 || h <= 0) return "—";
  return `${w} × ${h}`;
}
