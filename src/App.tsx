import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Toolbar from "./components/Toolbar";
import ImageGrid from "./components/ImageGrid";
import DetailPanel from "./components/DetailPanel";
import type { AppInfo, GetImagesResult, ImageItem, ScanProgress } from "./types";
import "./App.css";

const PAGE_SIZE = 60;

function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [libraryPath, setLibraryPath] = useState<string | null>(null);
  const [scanState, setScanState] = useState<ScanProgress | null>(null);
  const [images, setImages] = useState<ImageItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadingRef = useRef(false);
  const loadGen = useRef(0);
  const lastOffsetRef = useRef(0);

  // 加载一页图片。reset=true 时清空旧列表（换库/扫描完成）；追加加载互相排他。
  async function loadPage(offset: number, reset: boolean) {
    if (loadingRef.current && !reset) return;
    const gen = ++loadGen.current;
    loadingRef.current = true;
    setLoading(true);
    try {
      const res = await invoke<GetImagesResult>("get_images", { offset, limit: PAGE_SIZE });
      if (gen !== loadGen.current) return;
      setTotal(res.total);
      setImages((prev) => (reset ? res.items : [...prev, ...res.items]));
      if (res.items.length === 0) {
        lastOffsetRef.current = res.total; // 空页：标记已到底，避免无限追加
      } else {
        lastOffsetRef.current = offset + res.items.length;
      }
      if (reset) setSelectedId(null);
    } catch (e) {
      if (gen === loadGen.current) setError(String(e));
    } finally {
      if (gen === loadGen.current) {
        loadingRef.current = false;
        setLoading(false);
      }
    }
  }

  // 启动：拉取应用信息与上次图库路径（M0 保留）；有图库则加载首页
  useEffect(() => {
    invoke<AppInfo>("get_app_info")
      .then(setAppInfo)
      .catch((e) => setError(String(e)));
    invoke<string | null>("get_library")
      .then((path) => {
        setLibraryPath(path);
        if (path) void loadPage(0, true);
      })
      .catch((e) => setError(String(e)));
  }, []);

  // 监听扫描进度；扫描完成时刷新列表
  useEffect(() => {
    const unlisten = listen<ScanProgress>("scan-progress", (event) => {
      const p = event.payload;
      setScanState(p);
      if (p.phase === "done") {
        void loadPage(0, true);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // 选目录 → 立即持久化 → 回显并加载新库首页
  async function chooseLibrary() {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (dir) {
        const path = await invoke<string>("set_library", { path: dir });
        setLibraryPath(path);
        setScanState(null);
        setImages([]);
        setTotal(0);
        setSelectedId(null);
        void loadPage(0, true);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  // 启动扫描；false 表示已有扫描在跑
  async function startScan() {
    setError(null);
    try {
      const started = await invoke<boolean>("scan_library");
      if (!started) {
        setScanState({ phase: "error", scanned: 0, total: 0, message: "已有扫描正在进行中，请稍候" });
      }
    } catch (e) {
      setError(String(e));
    }
  }

  // 无限滚动：追加下一页
  function loadMore() {
    if (loading || images.length >= total || lastOffsetRef.current >= total) return;
    void loadPage(images.length, false);
  }

  return (
    <div className="app">
      <Toolbar
        appName={appInfo?.name ?? "Image Organizer"}
        appVersion={appInfo?.version ?? ""}
        libraryPath={libraryPath}
        scanState={scanState}
        onChooseLibrary={chooseLibrary}
        onScan={startScan}
      />
      {error && <div className="app-error">{error}</div>}
      <main className="main-area">
        {!libraryPath ? (
          <div className="empty-state">
            <p>尚未设置图库目录</p>
            <p className="hint">点击右上角「选择图库目录」按钮，选择要浏览的图片所在文件夹。</p>
          </div>
        ) : images.length === 0 ? (
          loading ? (
            <div className="empty-state">
              <p className="hint">加载中…</p>
            </div>
          ) : scanState?.phase === "scanning" ? (
            <div className="empty-state">
              <p className="hint">正在扫描图库…</p>
            </div>
          ) : scanState?.phase === "done" ? (
            <div className="empty-state">
              <p>暂无图片</p>
            </div>
          ) : scanState?.phase === "error" ? (
            <div className="empty-state">
              <p className="error">{scanState.message ?? "扫描出错"}</p>
            </div>
          ) : (
            <div className="empty-state">
              <p>尚未扫描图库</p>
              <p className="hint">点击「扫描」按钮开始索引当前目录。</p>
            </div>
          )
        ) : (
          <>
            <ImageGrid
              items={images}
              total={total}
              loading={loading}
              selectedId={selectedId}
              onLoadMore={loadMore}
              onSelect={setSelectedId}
            />
            {loading && <div className="grid-footer">加载中…</div>}
          </>
        )}
      </main>
      {selectedId !== null && (
        <DetailPanel key={selectedId} id={selectedId} onClose={() => setSelectedId(null)} />
      )}
    </div>
  );
}

export default App;