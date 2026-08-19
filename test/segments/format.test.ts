// 前端纯逻辑测试段：src/lib/format.ts 格式化函数。
// 断言可验证事实；formatDate 期望值由本地时间组件拼出，避免时区差异。

import { describe, expect, it } from "vitest";
import { formatDate, formatDimensions, formatFileSize } from "../../src/lib/format";

describe("formatFileSize", () => {
  it("0 字节显示为 0 B", () => {
    expect(formatFileSize(0)).toBe("0 B");
  });

  it("1023 字节显示为 1023 B（不足 1KB 不换算）", () => {
    expect(formatFileSize(1023)).toBe("1023 B");
  });

  it("1024 字节显示为 1.0 KB", () => {
    expect(formatFileSize(1024)).toBe("1.0 KB");
  });

  it("1536 字节显示为 1.5 KB（保留 1 位小数）", () => {
    expect(formatFileSize(1536)).toBe("1.5 KB");
  });

  it("1048576 字节显示为 1.0 MB", () => {
    expect(formatFileSize(1048576)).toBe("1.0 MB");
  });

  it("null 显示为 —", () => {
    expect(formatFileSize(null)).toBe("—");
  });

  it("负数显示为 —", () => {
    expect(formatFileSize(-1)).toBe("—");
  });
});

describe("formatDate", () => {
  it("0 显示为 —", () => {
    expect(formatDate(0)).toBe("—");
  });

  it("null 显示为 —", () => {
    expect(formatDate(null)).toBe("—");
  });

  it("秒级时间戳输出 YYYY-MM-DD HH:mm（本地时区）", () => {
    const epochSecs = 1700000000;
    const d = new Date(epochSecs * 1000);
    const pad = (n: number): string => String(n).padStart(2, "0");
    const expected = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
    expect(formatDate(epochSecs)).toBe(expected);
    expect(formatDate(epochSecs)).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });
});

describe("formatDimensions", () => {
  it("1920 × 1080 输出 1920 × 1080", () => {
    expect(formatDimensions(1920, 1080)).toBe("1920 × 1080");
  });

  it("任一为 null 显示为 —", () => {
    expect(formatDimensions(null, 1080)).toBe("—");
    expect(formatDimensions(1920, null)).toBe("—");
  });

  it("任一为 0 显示为 —", () => {
    expect(formatDimensions(0, 0)).toBe("—");
    expect(formatDimensions(1920, 0)).toBe("—");
  });
});