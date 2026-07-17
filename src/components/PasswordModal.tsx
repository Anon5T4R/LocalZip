import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { useUi } from "../state/ui";
import { useZip } from "../state/store";

/** Pede a senha pra extrair um zip cifrado (AES/ZipCrypto). */
export default function PasswordModal() {
  const ask = useUi((s) => s.passwordAsk);
  const setAsk = useUi((s) => s.setPasswordAsk);
  const [pw, setPw] = useState("");
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (ask) {
      setPw("");
      setTimeout(() => ref.current?.focus(), 0);
    }
  }, [ask]);

  if (!ask) return null;

  const submit = () => {
    if (!pw) return;
    setAsk(null);
    void useZip.getState().startExtract(ask.dest, ask.paths, pw);
  };

  return (
    <div className="modal-backdrop" onClick={() => setAsk(null)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{t("password.title")}</h2>
        <p className="muted small">{t("password.sub")}</p>
        <input
          ref={ref}
          type="password"
          value={pw}
          spellCheck={false}
          onChange={(e) => setPw(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
            if (e.key === "Escape") setAsk(null);
          }}
        />
        <div className="modal-actions">
          <button onClick={() => setAsk(null)}>{t("dlg.cancel")}</button>
          <button className="primary" disabled={!pw} onClick={submit}>
            {t("top.extractAll")}
          </button>
        </div>
      </div>
    </div>
  );
}
