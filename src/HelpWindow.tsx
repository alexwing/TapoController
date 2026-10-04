import { useState, useMemo, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { HELP_MARKDOWN } from "./helpContent";
import { Lang, resolveLang } from "./i18n";
import { api } from "./tauri";

export function HelpWindow({
  initialLang,
  onClose,
}: {
  initialLang?: Lang;
  onClose?: () => void;
}) {
  const [lang, setLang] = useState<Lang>(() => initialLang ?? resolveLang("system"));

  useEffect(() => {
    // Synchronize language with current app settings
    api
      .getConfig()
      .then((cfg) => {
        if (cfg?.language) {
          setLang(resolveLang(cfg.language));
        }
      })
      .catch(() => {});
  }, [initialLang]);

  useEffect(() => {
    // Re-check app language when the help window is focused
    const handleFocus = () => {
      api
        .getConfig()
        .then((cfg) => {
          if (cfg?.language) {
            setLang(resolveLang(cfg.language));
          }
        })
        .catch(() => {});
    };
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, []);

  const content = useMemo(
    () => HELP_MARKDOWN[lang] ?? HELP_MARKDOWN.es ?? HELP_MARKDOWN.en ?? "",
    [lang]
  );

  const handleClose = () => {
    if (onClose) {
      onClose();
      return;
    }
    try {
      getCurrentWindow().hide();
    } catch {
      window.close();
    }
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        handleClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div className="help-page">
      {onClose && (
        <button
          className="help-modal-close-btn"
          onClick={handleClose}
          title="Cerrar / Close (Esc)"
        >
          ✕
        </button>
      )}
      <main className="help-content prose-readable">
        <ReactMarkdown>{content}</ReactMarkdown>
      </main>
    </div>
  );
}
