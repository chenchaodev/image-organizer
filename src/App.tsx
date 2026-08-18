import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface AppInfo {
  name: string;
  version: string;
}

function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [libraryPath, setLibraryPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // 启动时拉取应用信息与上次图库路径：前者证明 IPC 链路通（M0 验收点），
  // 后者让重开应用后能恢复显示已选目录（settings 持久化回显）。
  useEffect(() => {
    invoke<AppInfo>("get_app_info")
      .then(setAppInfo)
      .catch((e) => setError(String(e)));
    invoke<string | null>("get_library")
      .then(setLibraryPath)
      .catch((e) => setError(String(e)));
  }, []);

  // 选目录 → 立即持久化 → 回显。选路径后的 set 失败会留在 error 区可见。
  async function chooseLibrary() {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (dir) {
        setLibraryPath(await invoke<string>("set_library", { path: dir }));
      }
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <main className="container">
      <h1>Image Organizer</h1>
      {appInfo && <p className="app-version">版本 {appInfo.version}</p>}
      <button type="button" onClick={chooseLibrary}>
        选择图库目录
      </button>
      {libraryPath ? (
        <p className="library-path">图库目录：{libraryPath}</p>
      ) : (
        <p className="hint">尚未设置图库目录</p>
      )}
      {error && <p className="error">{error}</p>}
    </main>
  );
}

export default App;
