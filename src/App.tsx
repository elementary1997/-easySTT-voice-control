import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import SettingsPanel from "./components/SettingsPanel";

export default function App() {
  useEffect(() => {
    const unsub = listen("show-settings-window", async () => {
      const win = getCurrentWindow();
      await win.unminimize();
      await win.show();
      await win.setFocus();
    });
    return () => { unsub.then((u) => u()); };
  }, []);

  return <SettingsPanel />;
}
