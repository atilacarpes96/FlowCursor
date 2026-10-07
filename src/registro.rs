// Registro de diagnóstico em flowcursor.log (recriado a cada início).
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static ARQUIVO: Mutex<Option<File>> = Mutex::new(None);
static INICIO: OnceLock<Instant> = OnceLock::new();

pub fn iniciar(caminho: &Path) {
    INICIO.get_or_init(Instant::now);
    if let Ok(f) = File::create(caminho) {
        *ARQUIVO.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }
}

pub fn escrever(msg: &str) {
    let t = INICIO.get_or_init(Instant::now).elapsed().as_secs_f64();
    let mut guarda = ARQUIVO.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = guarda.as_mut() {
        let _ = writeln!(f, "[{t:9.3}s] {msg}");
        let _ = f.flush();
    }
}

#[macro_export]
macro_rules! reg {
    ($($t:tt)*) => { $crate::registro::escrever(&format!($($t)*)) };
}
