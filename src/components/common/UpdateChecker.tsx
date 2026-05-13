import { useEffect, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { useI18n } from "@/lib/i18n";

export function UpdateChecker() {
  const { t } = useI18n();
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [version, setVersion] = useState("");
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState("");

  useEffect(() => {
    let cancelled = false;

    async function checkUpdate() {
      try {
        const update = await check();
        if (update && !cancelled) {
          setUpdateAvailable(true);
          setVersion(update.version);
        }
      } catch {
        // Silently ignore update check failures
      }
    }

    // Check after 3 seconds to not block startup
    const timer = setTimeout(checkUpdate, 3000);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);

  async function handleUpdate() {
    setInstalling(true);
    setProgress(t("update.downloading"));
    try {
      const update = await check();
      if (!update) return;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          setProgress(
            `${t("update.downloading")} (${(event.data.contentLength / 1024 / 1024).toFixed(1)} MB)`
          );
        } else if (event.event === "Finished") {
          setProgress(t("update.restart_hint"));
        }
      });
    } catch {
      setInstalling(false);
      setProgress("");
    }
  }

  if (!updateAvailable) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-center gap-3 rounded-lg border border-accent/30 bg-card px-4 py-3 shadow-lg">
      <div className="flex flex-col">
        <span className="text-sm font-medium text-text-primary">
          {t("update.available")} v{version}
        </span>
        {progress && (
          <span className="text-xs text-text-secondary">{progress}</span>
        )}
      </div>
      <div className="flex gap-2">
        {!installing && (
          <>
            <button
              onClick={() => setUpdateAvailable(false)}
              className="rounded px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary"
            >
              {t("update.later")}
            </button>
            <button
              onClick={handleUpdate}
              className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:bg-accent/80"
            >
              {t("update.now")}
            </button>
          </>
        )}
      </div>
    </div>
  );
}
