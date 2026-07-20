//! Bandeja + autostart + "fechar minimiza" (padrão da suíte; piloto no LocalClip).
//!
//! Por que o LocalZip precisa disso: extrair um arquivo grande (ou um volume
//! dividido, ou um RAR) leva minutos. Sem bandeja, fechar a janela mata o
//! processo — e com ele a thread de extração, sem aviso nenhum. Aqui a janela
//! some, o app continua vivo na bandeja e a extração termina.
//!
//! **A intenção do usuário mora no app** (`settings.json` na pasta de dados),
//! NÃO no registro do Windows. O registro é só o efeito, e é um efeito que se
//! perde sozinho: o `is_enabled()` do plugin de autostart só checa se a entrada
//! em `...\CurrentVersion\Run` EXISTE — nunca se ela aponta pro exe ATUAL. Se o
//! app for reinstalado noutro caminho, o plugin responde "ligado" e o app não
//! sobe mais no logon, com a checkbox marcada, calado. `reconcile()` roda no
//! boot e reimpõe o registro a partir da intenção guardada.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

// ---------- decisões puras (é o que os testes exercitam) ----------

/// O que o SO tem hoje, do ponto de vista de "precisa consertar?".
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OsAutostart {
    /// Entrada presente e apontando pro exe atual — nada a fazer.
    Ok,
    /// Ausente ou apontando pro caminho errado (app movido/reinstalado) — é o
    /// caso a reimpor, e o que o `is_enabled()` do plugin não enxerga.
    Broken,
    /// Desligado pelo Gerenciador de Tarefas do Windows. É escolha explícita do
    /// usuário, na UI oficial do SO: obedecemos e desmarcamos a checkbox.
    UserDisabled,
}

/// O que fazer com o registro depois de reconciliar.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Fix {
    Nothing,
    Enable,
    Disable,
}

/// Lê o estado do SO a partir dos dados CRUS do registro — sem tocar no
/// registro, pra poder testar o caso que motivou tudo isto: a entrada que
/// existe mas aponta pro caminho velho.
///
/// `run` = valor de `...\CurrentVersion\Run\<app>`; `approved` = bytes de
/// `...\StartupApproved\Run\<app>`; `expected` = `"<exe atual> --hidden"`.
pub fn os_state_from(run: Option<&str>, approved: Option<&[u8]>, expected: &str) -> OsAutostart {
    // Override do Gerenciador de Tarefas: 12 bytes = flag (DWORD) + FILETIME de
    // quando foi desligado. No flag, o bit 0 ligado = desabilitado; quando
    // habilitado, o timestamp fica zerado. Checamos os dois — o `auto-launch`
    // olha só o timestamp, o que não enxerga flag desligada com timestamp zero.
    if let Some(b) = approved {
        let flag_off = b.first().map(|f| f & 1 != 0).unwrap_or(false);
        let stamped_off = b.len() >= 12 && !b[4..12].iter().all(|x| *x == 0);
        if flag_off || stamped_off {
            return OsAutostart::UserDisabled;
        }
    }
    match run {
        Some(v) if v.trim().eq_ignore_ascii_case(expected.trim()) => OsAutostart::Ok,
        _ => OsAutostart::Broken,
    }
}

/// Casa a intenção guardada com o estado do SO. Devolve `(intenção efetiva,
/// conserto a aplicar)`.
///
/// A intenção NUNCA é lida do SO aqui: um registro quebrado não desmarca a
/// checkbox, ele é que é reescrito. A única coisa que muda a intenção é o
/// usuário — pela nossa UI ou pelo Gerenciador de Tarefas (senão brigaríamos
/// com ele todo boot).
pub fn reconcile_decision(intent: bool, state: OsAutostart) -> (bool, Fix) {
    if intent && state == OsAutostart::UserDisabled {
        return (false, Fix::Nothing);
    }
    match (intent, state) {
        (true, OsAutostart::Broken) => (true, Fix::Enable),
        (false, OsAutostart::Ok) => (false, Fix::Disable),
        _ => (intent, Fix::Nothing),
    }
}

/// O que fazer quando o usuário clica no X.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CloseAction {
    /// Fecha de verdade.
    Exit,
    /// Esconde na bandeja e segue vivo.
    ToTray,
}

/// Fechar com operação em andamento **nunca** mata a extração.
///
/// A opção "fechar minimiza pra bandeja" é opt-in e vem desligada — mas com uma
/// extração rodando, sair no X destruiria trabalho longo em silêncio. Aqui o
/// caso perde pra regra da suíte de não perder trabalho calado: vai pra bandeja
/// e a dica da bandeja diz que ainda está trabalhando.
pub fn decide_close(close_to_tray: bool, ops_running: bool) -> CloseAction {
    if close_to_tray || ops_running {
        CloseAction::ToTray
    } else {
        CloseAction::Exit
    }
}

// ---------- persistência da intenção ----------

/// Configurações que moram no backend (o tema/idioma seguem no localStorage do
/// front — estes precisam ser lidos no boot, ANTES de existir webview).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `None` = usuário nunca decidiu (instalação antiga): herda o que o SO já
    /// tem, pra não ligar nem desligar nada por conta própria.
    pub autostart: Option<bool>,
    pub close_to_tray: bool,
}

pub struct SettingsState(pub Mutex<Settings>);

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("settings.json"))
}

pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(path: &Path, s: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn read(app: &AppHandle) -> Settings {
    app.state::<SettingsState>().0.lock().unwrap().clone()
}

fn write(app: &AppHandle, s: Settings) -> Result<(), String> {
    *app.state::<SettingsState>().0.lock().unwrap() = s.clone();
    match settings_path(app) {
        Some(p) => save_settings(&p, &s),
        None => Err("pasta de dados indisponível".into()),
    }
}

// ---------- leitura do registro (a parte que não dá pra testar sem SO) ----------

#[cfg(windows)]
fn os_autostart(app: &AppHandle) -> OsAutostart {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    const RUN: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str =
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    let name = &app.package_info().name;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let approved = hkcu
        .open_subkey_with_flags(APPROVED, KEY_READ)
        .ok()
        .and_then(|k| k.get_raw_value(name).ok())
        .map(|v| v.bytes);
    let run = hkcu
        .open_subkey_with_flags(RUN, KEY_READ)
        .ok()
        .and_then(|k| k.get_value::<String, _>(name).ok());

    os_state_from(run.as_deref(), approved.as_deref(), &expected_command())
}

/// Espelha o formato que o `auto-launch` grava: `"<exe> <args>"`, sem aspas.
#[cfg(windows)]
fn expected_command() -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    format!("{exe} --hidden")
}

/// Fora do Windows não há registro pra envelhecer: o `is_enabled()` basta.
#[cfg(not(windows))]
fn os_autostart(app: &AppHandle) -> OsAutostart {
    if app.autolaunch().is_enabled().unwrap_or(false) {
        OsAutostart::Ok
    } else {
        OsAutostart::Broken
    }
}

/// Intenção guardada; sem decisão registrada, herda o SO uma única vez.
fn intent(app: &AppHandle) -> bool {
    read(app).autostart.unwrap_or_else(|| app.autolaunch().is_enabled().unwrap_or(false))
}

/// Alinha o SO com a intenção guardada, a cada boot.
pub fn reconcile(app: &AppHandle) {
    let state = os_autostart(app);
    let (want, fix) = reconcile_decision(intent(app), state);

    let mut s = read(app);
    s.autostart = Some(want);
    let _ = write(app, s);

    let mgr = app.autolaunch();
    let res = match fix {
        Fix::Nothing => Ok(()),
        Fix::Enable => mgr.enable(),
        Fix::Disable => mgr.disable(),
    };
    if let Err(e) = res {
        eprintln!("[localzip] falha ao reconciliar o autostart (want={want}, so={state:?}): {e}");
    }
}

// ---------- comandos ----------

#[tauri::command(async)]
pub fn autostart_get(app: AppHandle) -> Result<bool, String> {
    Ok(intent(&app))
}

#[tauri::command(async)]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    // A intenção primeiro: se o registro falhar, o reconcile do próximo boot
    // tenta de novo em vez de esquecer o que o usuário pediu.
    let mut s = read(&app);
    s.autostart = Some(enabled);
    write(&app, s)?;

    let mgr = app.autolaunch();
    if enabled {
        // NUNCA `disable().and_then(enable)`: o disable() erra quando não há
        // entrada, e o erro engoliria o enable.
        let _ = mgr.disable();
        mgr.enable().map_err(|e| e.to_string())
    } else {
        mgr.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command(async)]
pub fn close_to_tray_get(app: AppHandle) -> Result<bool, String> {
    Ok(read(&app).close_to_tray)
}

#[tauri::command(async)]
pub fn close_to_tray_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut s = read(&app);
    s.close_to_tray = enabled;
    write(&app, s)
}

// ---------- janela e bandeja ----------

pub fn open_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            open_main(app);
        }
    }
}

/// Itens do menu da bandeja, guardados pra poder trocar o texto quando o front
/// disser em que idioma está (ver `tray_labels_set`).
pub struct TrayMenu(pub Mutex<Option<(MenuItem<tauri::Wry>, MenuItem<tauri::Wry>)>>);

/// O menu da bandeja nasce em Rust, no `setup`, **antes de existir webview** —
/// então ele não tem como saber o idioma escolhido, que mora no localStorage do
/// front. O piloto (LocalClip) resolveu isso deixando os rótulos fixos em
/// português. Aqui o front manda os rótulos traduzidos assim que monta, e a
/// troca de idioma reenvia: a bandeja acompanha as Configurações de verdade.
#[tauri::command(async)]
pub fn tray_labels_set(app: AppHandle, show: String, quit: String) -> Result<(), String> {
    if let Some((s, q)) = app.state::<TrayMenu>().0.lock().unwrap().as_ref() {
        s.set_text(show).map_err(|e| e.to_string())?;
        q.set_text(quit).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Monta a bandeja e o gancho de fechar. `ops_running` diz se há operação viva
/// (é a `OpsState`, passada como closure pra este módulo não conhecer o motor).
///
/// Os rótulos aqui são só o texto do 1º instante, antes de o front se
/// apresentar; ficam em português porque é o idioma-fonte da suíte.
pub fn setup(
    app: &AppHandle,
    ops_running: impl Fn() -> bool + Send + Sync + 'static,
) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "toggle", "Mostrar/Ocultar", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    app.manage(TrayMenu(Mutex::new(Some((show.clone(), quit.clone())))));

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("LocalZip")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_main(app),
            // "Sair" SEMPRE fecha de verdade, mesmo com operação rodando: é o
            // único jeito de o usuário desistir de propósito.
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        })
        .build(app)?;

    if let Some(win) = app.get_webview_window("main") {
        let w = win.clone();
        let handle = app.clone();
        win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = read(&handle).close_to_tray;
                if decide_close(to_tray, ops_running()) == CloseAction::ToTray {
                    api.prevent_close();
                    let _ = w.hide();
                }
            }
        });
    }
    Ok(())
}

/// Logon com `--hidden`: abre direto na bandeja, mas só se "fechar minimiza"
/// estiver ligado — com a opção desligada o usuário fecharia no X e o app
/// morreria escondido sem servir pra nada.
pub fn hide_if_launched_hidden(app: &AppHandle) {
    if std::env::args().any(|a| a == "--hidden") && read(app).close_to_tray {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
        }
    }
}

pub fn init_state(app: &AppHandle) {
    let s = settings_path(app).map(|p| load_settings(&p)).unwrap_or_default();
    app.manage(SettingsState(Mutex::new(s)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOVO: &str = r"C:\Program Files\LocalZip\LocalZip.exe --hidden";
    const VELHO: &str = r"C:\Users\joao\AppData\Local\LocalZip\LocalZip.exe --hidden";

    #[test]
    fn caminho_atual_esta_ok() {
        assert_eq!(os_state_from(Some(NOVO), None, NOVO), OsAutostart::Ok);
    }

    #[test]
    fn caminho_de_exe_antigo_e_quebrado_nao_ligado() {
        // ESTE é o caso que o `is_enabled()` do plugin erra: a entrada EXISTE,
        // então ele diz "ligado" — e o app não sobe mais no logon.
        assert_eq!(os_state_from(Some(VELHO), None, NOVO), OsAutostart::Broken);
    }

    #[test]
    fn entrada_ausente_e_quebrada() {
        assert_eq!(os_state_from(None, None, NOVO), OsAutostart::Broken);
    }

    #[test]
    fn espaco_e_caixa_nao_contam() {
        let outro = r"c:\program files\localzip\localzip.exe --hidden  ";
        assert_eq!(os_state_from(Some(outro), None, NOVO), OsAutostart::Ok);
    }

    #[test]
    fn gerenciador_de_tarefas_desligado_por_flag() {
        // Flag com bit 0 ligado e timestamp zerado: o auto-launch, que só olha o
        // timestamp, não veria isto.
        let bytes = [3u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(os_state_from(Some(NOVO), Some(&bytes), NOVO), OsAutostart::UserDisabled);
    }

    #[test]
    fn gerenciador_de_tarefas_desligado_por_timestamp() {
        let bytes = [2u8, 0, 0, 0, 0xAB, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(os_state_from(Some(NOVO), Some(&bytes), NOVO), OsAutostart::UserDisabled);
    }

    #[test]
    fn gerenciador_de_tarefas_habilitado_nao_atrapalha() {
        let bytes = [2u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(os_state_from(Some(NOVO), Some(&bytes), NOVO), OsAutostart::Ok);
    }

    /// A prova pedida: exe mudou de lugar → a INTENÇÃO sobrevive e o registro é
    /// que é reescrito.
    #[test]
    fn intencao_sobrevive_a_exe_movido() {
        let estado = os_state_from(Some(VELHO), None, NOVO);
        let (want, fix) = reconcile_decision(true, estado);
        assert!(want, "a intenção do usuário não pode ser derrubada por registro obsoleto");
        assert_eq!(fix, Fix::Enable, "o registro velho tem que ser reimposto");
    }

    #[test]
    fn gerenciador_de_tarefas_vence_a_checkbox() {
        let (want, fix) = reconcile_decision(true, OsAutostart::UserDisabled);
        assert!(!want);
        assert_eq!(fix, Fix::Nothing, "não brigar com a UI oficial do Windows todo boot");
    }

    #[test]
    fn desligado_limpa_entrada_existente() {
        assert_eq!(reconcile_decision(false, OsAutostart::Ok), (false, Fix::Disable));
        assert_eq!(reconcile_decision(false, OsAutostart::Broken), (false, Fix::Nothing));
    }

    #[test]
    fn ligado_e_ok_nao_mexe_no_registro() {
        assert_eq!(reconcile_decision(true, OsAutostart::Ok), (true, Fix::Nothing));
    }

    #[test]
    fn fechar_com_extracao_rodando_vai_pra_bandeja() {
        assert_eq!(decide_close(false, true), CloseAction::ToTray);
        assert_eq!(decide_close(true, true), CloseAction::ToTray);
        assert_eq!(decide_close(true, false), CloseAction::ToTray);
        assert_eq!(decide_close(false, false), CloseAction::Exit);
    }

    #[test]
    fn settings_ida_e_volta_no_disco() {
        let p = std::env::temp_dir().join("localzip-tray-test-settings.json");
        let _ = std::fs::remove_file(&p);
        // Arquivo inexistente = tudo no default, sem decisão de autostart.
        assert_eq!(load_settings(&p).autostart, None);

        let s = Settings { autostart: Some(true), close_to_tray: true };
        save_settings(&p, &s).unwrap();
        let lido = load_settings(&p);
        assert_eq!(lido.autostart, Some(true));
        assert!(lido.close_to_tray);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn settings_corrompido_nao_derruba_o_app() {
        let p = std::env::temp_dir().join("localzip-tray-test-corrompido.json");
        std::fs::write(&p, b"{ isto nao e json").unwrap();
        assert_eq!(load_settings(&p).autostart, None);
        let _ = std::fs::remove_file(&p);
    }
}
