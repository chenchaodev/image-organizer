import type { ScanProgress } from "../types";

interface ToolbarProps {
  appName: string;
  appVersion: string;
  libraryPath: string | null;
  scanState: ScanProgress | null;
  onChooseLibrary: () => void;
  onScan: () => void;
}

export default function Toolbar({
  appName,
  appVersion,
  libraryPath,
  scanState,
  onChooseLibrary,
  onScan,
}: ToolbarProps) {
  const scanning = scanState?.phase === "scanning";
  const percent =
    scanState && scanState.total > 0
      ? Math.min(100, Math.round((scanState.scanned / scanState.total) * 100))
      : 0;

  return (
    <header className="toolbar">
      <div className="toolbar-row">
        <div className="toolbar-brand">
          <span className="brand-name">{appName}</span>
          {appVersion && <span className="brand-version">版本 {appVersion}</span>}
        </div>
        <div className="toolbar-path" title={libraryPath ?? undefined}>
          {libraryPath ?? "未设置图库目录"}
        </div>
        <div className="toolbar-actions">
          <button type="button" onClick={onChooseLibrary}>
            选择图库目录
          </button>
          <button type="button" onClick={onScan} disabled={!libraryPath || scanning}>
            {scanning ? "扫描中…" : "扫描"}
          </button>
        </div>
      </div>
      {scanState && (
        <div className="scan-status">
          {scanState.phase === "scanning" ? (
            <>
              <div className="progress-track">
                <div className="progress-fill" style={{ width: `${percent}%` }} />
              </div>
              <span className="progress-text">
                {scanState.scanned} / {scanState.total}（{percent}%）
              </span>
            </>
          ) : (
            <span className={scanState.phase === "error" ? "scan-message error" : "scan-message"}>
              {scanState.message ?? (scanState.phase === "done" ? "扫描完成" : "")}
            </span>
          )}
        </div>
      )}
    </header>
  );
}
