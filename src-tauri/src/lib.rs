mod archive;
mod rar;
mod split;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

use archive::ArchiveInfo;

/// Operações em andamento (op_id → flag de cancelamento) — mesmo padrão do
/// LocalFiles.
#[derive(Default)]
pub struct OpsState {
    ops: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    next_id: AtomicU64,
}

impl OpsState {
    fn register(&self) -> (u64, Arc<AtomicBool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let flag = Arc::new(AtomicBool::new(false));
        self.ops.lock().unwrap().insert(id, flag.clone());
        (id, flag)
    }
    fn cancel(&self, id: u64) {
        if let Some(f) = self.ops.lock().unwrap().get(&id) {
            f.store(true, Ordering::Relaxed);
        }
    }
    fn finish(&self, id: u64) {
        self.ops.lock().unwrap().remove(&id);
    }
}

/// Lê o índice do arquivo compactado (sem extrair nada).
#[tauri::command(async)]
fn open_archive(path: String) -> Result<ArchiveInfo, String> {
    archive::open_archive(&path)
}

/// Extrai tudo (`paths` = null) ou os itens selecionados pro destino.
/// Progresso/fim via `zipop-progress`/`zipop-done`.
#[tauri::command(async)]
fn start_extract(
    app: AppHandle,
    state: State<'_, OpsState>,
    archive: String,
    dest: String,
    paths: Option<Vec<String>>,
    password: Option<String>,
) -> Result<u64, String> {
    let (op_id, cancel) = state.register();
    let handle = app.clone();
    std::thread::spawn(move || {
        archive::extract(&handle, op_id, cancel, archive, dest, paths, password);
        handle.state::<OpsState>().finish(op_id);
    });
    Ok(op_id)
}

/// Cria um arquivo novo (`format`: "zip" | "targz"; senha opcional só no zip).
#[tauri::command(async)]
fn start_create(
    app: AppHandle,
    state: State<'_, OpsState>,
    dest: String,
    format: String,
    sources: Vec<String>,
    password: Option<String>,
) -> Result<u64, String> {
    let (op_id, cancel) = state.register();
    let handle = app.clone();
    std::thread::spawn(move || {
        archive::create(&handle, op_id, cancel, dest, format, sources, password);
        handle.state::<OpsState>().finish(op_id);
    });
    Ok(op_id)
}

/// Adiciona e/ou remove itens de um zip existente SEM re-extrair o resto.
/// `add` = caminhos no disco; `remove` = caminhos DENTRO do arquivo.
#[tauri::command(async)]
fn start_update(
    app: AppHandle,
    state: State<'_, OpsState>,
    archive: String,
    add: Vec<String>,
    remove: Vec<String>,
    password: Option<String>,
) -> Result<u64, String> {
    let (op_id, cancel) = state.register();
    let handle = app.clone();
    std::thread::spawn(move || {
        archive::update(&handle, op_id, cancel, archive, add, remove, password);
        handle.state::<OpsState>().finish(op_id);
    });
    Ok(op_id)
}

/// Testa a integridade lendo tudo (valida CRC no zip; trunca/corrompe no tar).
#[tauri::command(async)]
fn test_integrity(archive: String, password: Option<String>) -> archive::IntegrityResult {
    archive::test_integrity(&archive, password.as_deref())
}

#[tauri::command(async)]
fn cancel_op(state: State<'_, OpsState>, op_id: u64) {
    state.cancel(op_id);
}

/// Arquivo passado no launch (associação/abrir com), se houver.
#[tauri::command(async)]
fn get_startup_file() -> Option<String> {
    startup_file_from(std::env::args().skip(1))
}

fn startup_file_from(args: impl Iterator<Item = String>) -> Option<String> {
    args.filter(|a| !a.starts_with('-'))
        .find(|a| Path::new(a).is_file())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // Segunda instância: foca a janela e abre o arquivo que veio no arg.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
            if let Some(file) = startup_file_from(args.into_iter().skip(1)) {
                let _ = app.emit("open-file", file);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(OpsState::default())
        .invoke_handler(tauri::generate_handler![
            open_archive,
            start_extract,
            start_create,
            start_update,
            test_integrity,
            cancel_op,
            get_startup_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_file_ignora_flags_e_pastas() {
        let dir = std::env::temp_dir();
        let f = dir.join("localzip-startup-test.zip");
        std::fs::write(&f, b"x").unwrap();
        let fs_str = f.to_string_lossy().into_owned();
        let args = vec!["--flag".to_string(), dir.to_string_lossy().into_owned(), fs_str.clone()];
        assert_eq!(startup_file_from(args.into_iter()), Some(fs_str));
        let _ = std::fs::remove_file(&f);
    }
}
