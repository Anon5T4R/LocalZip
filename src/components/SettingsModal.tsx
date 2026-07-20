import { useEffect, useState } from "react";
import {
  autostartGet,
  autostartSet,
  closeToTrayGet,
  closeToTraySet,
  isTauri,
} from "../lib/backend";
import { LOCALE_LABELS, setLocale, t, useLocale, type Locale } from "../lib/i18n";
import { useUi, type Theme } from "../state/ui";

/** Configurações: tema, idioma e segundo plano (padrão da suíte). */
export default function SettingsModal() {
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const pushToast = useUi((s) => s.pushToast);
  const locale = useLocale();

  // Estas duas moram no backend, não no localStorage: a intenção de autostart
  // precisa ser lida no boot, antes de existir webview. Sempre relemos ao abrir
  // — o reconcile do boot pode ter desmarcado (Gerenciador de Tarefas).
  const [autostart, setAutostart] = useState(false);
  const [closeToTray, setCloseToTray] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open || !isTauri) return;
    void autostartGet().then(setAutostart).catch(() => {});
    void closeToTrayGet().then(setCloseToTray).catch(() => {});
  }, [open]);

  if (!open) return null;

  const toggleAutostart = (v: boolean) => {
    // Otimista, mas com rollback: se o registro recusar, a checkbox não pode
    // ficar mentindo que está ligado.
    setAutostart(v);
    setBusy(true);
    autostartSet(v)
      .catch((e) => {
        setAutostart(!v);
        pushToast("error", t("toast.settingsFailed", { error: String(e) }));
      })
      .finally(() => setBusy(false));
  };

  const toggleCloseToTray = (v: boolean) => {
    setCloseToTray(v);
    setBusy(true);
    closeToTraySet(v)
      .catch((e) => {
        setCloseToTray(!v);
        pushToast("error", t("toast.settingsFailed", { error: String(e) }));
      })
      .finally(() => setBusy(false));
  };

  const themes: { value: Theme; label: string }[] = [
    { value: "system", label: t("settings.themeSystem") },
    { value: "light", label: t("settings.themeLight") },
    { value: "dark", label: t("settings.themeDark") },
    { value: "nature", label: t("settings.themeNature") },
    { value: "darkblue", label: t("settings.themeDarkBlue") },
    { value: "calmgreen", label: t("settings.themeCalmGreen") },
    { value: "pastelpink", label: t("settings.themePastelPink") },
    { value: "punkprincess", label: t("settings.themePunkPrincess") },
  ];

  return (
    <div className="modal-backdrop" onClick={() => setOpen(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{t("settings.title")}</h2>

        <div className="settings-row">
          <span>{t("settings.theme")}</span>
          <div className="segmented">
            {themes.map((th) => (
              <button
                key={th.value}
                className={theme === th.value ? "active" : ""}
                onClick={() => setTheme(th.value)}
              >
                {th.label}
              </button>
            ))}
          </div>
        </div>

        <div className="settings-row">
          <span>{t("settings.language")}</span>
          <div className="segmented">
            {(Object.keys(LOCALE_LABELS) as Locale[]).map((l) => (
              <button key={l} className={locale === l ? "active" : ""} onClick={() => setLocale(l)}>
                {LOCALE_LABELS[l]}
              </button>
            ))}
          </div>
        </div>

        {isTauri && (
          <>
            <h3 className="settings-section">{t("settings.background")}</h3>

            <div className="settings-row">
              <span>
                {t("settings.closeToTray")}
                <span className="muted small settings-hint">{t("settings.closeToTrayHint")}</span>
              </span>
              <input
                type="checkbox"
                checked={closeToTray}
                disabled={busy}
                onChange={(e) => toggleCloseToTray(e.target.checked)}
              />
            </div>

            <div className="settings-row">
              <span>
                {t("settings.autostart")}
                <span className="muted small settings-hint">{t("settings.autostartHint")}</span>
              </span>
              <input
                type="checkbox"
                checked={autostart}
                disabled={busy}
                onChange={(e) => toggleAutostart(e.target.checked)}
              />
            </div>
          </>
        )}

        <p className="muted about">
          <strong>LocalZip</strong>
          {t("settings.about")}
        </p>

        <div className="modal-actions">
          <button className="primary" onClick={() => setOpen(false)}>
            {t("dlg.ok")}
          </button>
        </div>
      </div>
    </div>
  );
}
