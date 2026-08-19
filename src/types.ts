// IPC 契约类型：与 src-tauri 命令返回结构一一对应。

export interface AppInfo {
  name: string;
  version: string;
}

export interface ImageItem {
  id: number;
  path: string;
  width: number | null;
  height: number | null;
  format: string | null;
  fileSize: number | null;
  mtime: number | null;
}

export interface ImageDetail extends ImageItem {
  exif: Record<string, string>;
}

export interface GetImagesResult {
  items: ImageItem[];
  total: number;
}

export type ScanPhase = "scanning" | "done" | "error";

export interface ScanProgress {
  phase: ScanPhase;
  scanned: number;
  total: number;
  message?: string;
}
