// Instalação, atualização e desinstalação, sem pedir administrador.
// O mesmo .exe vira instalador quando o nome tem "instalador" (FlowCursor-Instalador.exe)
// e desinstala com --desinstalar (é o que Configurações > Aplicativos chama).
use crate::ffi::*;
use crate::sistema;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use std::time::Duration;

const VERSAO: &str = env!("CARGO_PKG_VERSION");
const CHAVE_DESINSTALAR: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\FlowCursor";
const NOME_ATALHO: &str = "FlowCursor.lnk";
const DESCRICAO: &str = "FlowCursor: ponteiro com mola, borrão e rastro";
const BOTAO_PRINCIPAL: i32 = 100;

pub fn e_instalador(args: &[String]) -> bool {
    if args.iter().any(|a| a == "--instalar") {
        return true;
    }
    let nome = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    ["instalador", "setup", "install"].iter().any(|k| nome.contains(k))
}

// ---------- Pastas ----------

fn pasta_conhecida(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let mut p: *mut u16 = null_mut();
        if SHGetKnownFolderPath(id, 0, 0, &mut p) < 0 || p.is_null() {
            return None;
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        CoTaskMemFree(p.cast());
        Some(PathBuf::from(s))
    }
}

/// %LOCALAPPDATA%\Programs\FlowCursor: a pasta de programas do próprio usuário.
pub fn pasta_instalacao() -> PathBuf {
    pasta_conhecida(&FOLDERID_USER_PROGRAM_FILES)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Programs")))
        .unwrap_or_else(|| PathBuf::from(r"C:\FlowCursor"))
        .join("FlowCursor")
}

/// Atalhos na área de trabalho e no menu Iniciar.
fn atalhos() -> Vec<PathBuf> {
    [FOLDERID_DESKTOP, FOLDERID_PROGRAMS].iter().filter_map(pasta_conhecida).map(|p| p.join(NOME_ATALHO)).collect()
}

// ---------- Atalho (.lnk) pela interface COM do Windows ----------

/// Pega o método `indice` da tabela virtual de uma interface COM.
unsafe fn metodo<F: Copy>(obj: *mut c_void, indice: usize) -> F {
    let tabela = *(obj as *const *const usize);
    std::mem::transmute_copy(&*tabela.add(indice))
}

unsafe fn liberar(obj: *mut c_void) {
    let release: unsafe extern "system" fn(*mut c_void) -> u32 = metodo(obj, 2);
    release(obj);
}

fn criar_atalho(lnk: &Path, alvo: &Path, args: &str) -> bool {
    type Texto = unsafe extern "system" fn(*mut c_void, *const u16) -> i32;
    type Icone = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;
    type Consulta = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
    type Salvar = unsafe extern "system" fn(*mut c_void, *const u16, BOOL) -> i32;
    unsafe {
        let mut link: *mut c_void = null_mut();
        if CoCreateInstance(&CLSID_SHELL_LINK, null_mut(), CLSCTX_INPROC_SERVER, &IID_ISHELL_LINK_W, &mut link) < 0 || link.is_null() {
            return false;
        }
        let alvo_w = w(&alvo.to_string_lossy());
        let pasta_w = w(&alvo.parent().unwrap_or(Path::new("")).to_string_lossy());
        let args_w = w(args);
        let descricao_w = w(DESCRICAO);
        // índices da IShellLinkW: 7 SetDescription, 9 SetWorkingDirectory, 11 SetArguments,
        // 17 SetIconLocation, 20 SetPath
        metodo::<Texto>(link, 20)(link, alvo_w.as_ptr());
        metodo::<Texto>(link, 11)(link, args_w.as_ptr());
        metodo::<Texto>(link, 9)(link, pasta_w.as_ptr());
        metodo::<Texto>(link, 7)(link, descricao_w.as_ptr());
        metodo::<Icone>(link, 17)(link, alvo_w.as_ptr(), 0);
        let mut arquivo: *mut c_void = null_mut();
        metodo::<Consulta>(link, 0)(link, &IID_IPERSIST_FILE, &mut arquivo);
        let mut ok = false;
        if !arquivo.is_null() {
            // IPersistFile: 6 Save
            ok = metodo::<Salvar>(arquivo, 6)(arquivo, w(&lnk.to_string_lossy()).as_ptr(), 1) >= 0;
            liberar(arquivo);
        }
        liberar(link);
        ok
    }
}

// ---------- Registro (Configurações > Aplicativos) ----------

fn reg_texto(chave: HANDLE, nome: &str, valor: &str) {
    let v = w(valor);
    unsafe {
        RegSetValueExW(chave, w(nome).as_ptr(), 0, REG_SZ, v.as_ptr().cast(), (v.len() * 2) as u32);
    }
}

fn reg_numero(chave: HANDLE, nome: &str, valor: u32) {
    unsafe {
        RegSetValueExW(chave, w(nome).as_ptr(), 0, REG_DWORD, (&valor as *const u32).cast(), 4);
    }
}

fn registrar_desinstalador(exe: &Path, pasta: &Path) -> bool {
    unsafe {
        let mut chave: HANDLE = 0;
        if RegCreateKeyExW(HKEY_CURRENT_USER, w(CHAVE_DESINSTALAR).as_ptr(), 0, null(), 0, KEY_ALL_ACCESS, null(), &mut chave, null_mut()) != 0 {
            return false;
        }
        let e = exe.display().to_string();
        reg_texto(chave, "DisplayName", "FlowCursor");
        reg_texto(chave, "DisplayVersion", VERSAO);
        reg_texto(chave, "Publisher", "FlowCursor");
        reg_texto(chave, "DisplayIcon", &format!("{e},0"));
        reg_texto(chave, "InstallLocation", &pasta.display().to_string());
        reg_texto(chave, "UninstallString", &format!("\"{e}\" --desinstalar"));
        reg_texto(chave, "QuietUninstallString", &format!("\"{e}\" --desinstalar --silencioso"));
        reg_numero(chave, "NoModify", 1);
        reg_numero(chave, "NoRepair", 1);
        reg_numero(chave, "EstimatedSize", std::fs::metadata(exe).map(|m| (m.len() / 1024) as u32).unwrap_or(0));
        RegCloseKey(chave);
        true
    }
}

fn versao_instalada() -> Option<String> {
    unsafe {
        let mut chave: HANDLE = 0;
        if RegOpenKeyExW(HKEY_CURRENT_USER, w(CHAVE_DESINSTALAR).as_ptr(), 0, KEY_QUERY_VALUE, &mut chave) != 0 {
            return None;
        }
        let mut buf = [0u16; 64];
        let mut tam = (buf.len() * 2) as u32;
        let ok = RegQueryValueExW(chave, w("DisplayVersion").as_ptr(), null_mut(), null_mut(), buf.as_mut_ptr().cast(), &mut tam) == 0;
        RegCloseKey(chave);
        ok.then(|| ler_w(&buf))
    }
}

// ---------- Diálogo nativo ----------

struct Dialogo<'a> {
    instrucao: &'a str,
    conteudo: &'a str,
    /// Botões grandes ("command links"); a 1ª linha é o título e a 2ª a explicação.
    botoes: &'a [(i32, &'a str)],
    comuns: u32,
    verificacao: Option<(&'a str, bool)>,
    detalhes: Option<&'a str>,
    rodape: Option<&'a str>,
    erro: bool,
}

fn icone_programa() -> HANDLE {
    unsafe {
        let lado = GetSystemMetrics(SM_CXICON).max(32);
        LoadImageW(GetModuleHandleW(null()), 1usize as *const u16, IMAGE_ICON, lado, lado, 0)
    }
}

/// Mostra o diálogo e devolve (botão escolhido, caixa de verificação marcada).
fn dialogo(d: &Dialogo) -> (i32, bool) {
    unsafe {
        // Carregado na hora: sem o manifesto dos controles novos a função não existe,
        // e aí vale uma caixa de mensagem simples.
        let comctl = LoadLibraryW(w("comctl32.dll").as_ptr());
        let f = if comctl != 0 { GetProcAddress(comctl, c"TaskDialogIndirect".as_ptr().cast()) } else { null() };
        if f.is_null() {
            let texto = format!("{}\n\n{}", d.instrucao, d.conteudo);
            let cancelavel = d.botoes.iter().any(|b| b.0 == IDCANCEL);
            let r = MessageBoxW(0, w(&texto).as_ptr(), w("FlowCursor").as_ptr(), if cancelavel { MB_OKCANCEL } else { MB_OK } | MB_ICONINFORMATION);
            let botao = if r == IDOK { d.botoes.first().map_or(IDOK, |b| b.0) } else { IDCANCEL };
            return (botao, d.verificacao.is_some_and(|v| v.1));
        }
        let f: TaskDialogIndirectFn = std::mem::transmute(f);
        let titulo = w("FlowCursor");
        let instrucao = w(d.instrucao);
        let conteudo = w(d.conteudo);
        let textos: Vec<Vec<u16>> = d.botoes.iter().map(|b| w(b.1)).collect();
        let botoes: Vec<TASKDIALOG_BUTTON> =
            d.botoes.iter().zip(&textos).map(|(b, t)| TASKDIALOG_BUTTON { nButtonID: b.0, pszButtonText: t.as_ptr() }).collect();
        let verificacao = d.verificacao.map(|v| w(v.0));
        let detalhes = d.detalhes.map(w);
        let rodape = d.rodape.map(w);
        let (mostrar, ocultar) = (w("Mostrar detalhes"), w("Ocultar detalhes"));
        let ptr = |v: &Option<Vec<u16>>| v.as_ref().map_or(null(), |x| x.as_ptr());
        let mut flags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT;
        if !botoes.is_empty() {
            flags |= TDF_USE_COMMAND_LINKS;
        }
        if d.verificacao.is_some_and(|v| v.1) {
            flags |= TDF_VERIFICATION_FLAG_CHECKED;
        }
        let icone = if d.erro { 0 } else { icone_programa() };
        let icone_principal = if d.erro {
            TD_ERROR_ICON
        } else if icone != 0 {
            flags |= TDF_USE_HICON_MAIN;
            icone
        } else {
            TD_INFORMATION_ICON
        };
        let cfg = TASKDIALOGCONFIG {
            cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
            hwndParent: 0,
            hInstance: GetModuleHandleW(null()),
            dwFlags: flags,
            dwCommonButtons: d.comuns,
            pszWindowTitle: titulo.as_ptr(),
            hMainIcon: icone_principal,
            pszMainInstruction: instrucao.as_ptr(),
            pszContent: conteudo.as_ptr(),
            cButtons: botoes.len() as u32,
            pButtons: if botoes.is_empty() { null() } else { botoes.as_ptr() },
            nDefaultButton: d.botoes.first().map_or(0, |b| b.0),
            cRadioButtons: 0,
            pRadioButtons: null(),
            nDefaultRadioButton: 0,
            pszVerificationText: ptr(&verificacao),
            pszExpandedInformation: ptr(&detalhes),
            pszExpandedControlText: ocultar.as_ptr(),
            pszCollapsedControlText: mostrar.as_ptr(),
            hFooterIcon: 0,
            pszFooter: ptr(&rodape),
            pfCallback: 0,
            lpCallbackData: 0,
            cxWidth: 0,
        };
        let (mut botao, mut radio, mut marcado) = (0i32, 0i32, 0 as BOOL);
        let hr = f(&cfg, &mut botao, &mut radio, &mut marcado);
        if icone != 0 {
            DestroyIcon(icone);
        }
        if hr < 0 {
            return (IDCANCEL, false);
        }
        (botao, marcado != 0)
    }
}

fn aviso_erro(instrucao: &str, conteudo: &str) {
    dialogo(&Dialogo {
        instrucao,
        conteudo,
        botoes: &[],
        comuns: TDCBF_CLOSE_BUTTON,
        verificacao: None,
        detalhes: None,
        rodape: None,
        erro: true,
    });
}

// ---------- Instalar ----------

fn copiar_e_registrar(pasta: &Path, destino: &Path, com_windows: bool) -> Result<(), String> {
    let origem = std::env::current_exe().map_err(|e| e.to_string())?;
    // Atualização: o FlowCursor aberto precisa fechar para o .exe ser trocado.
    sistema::fechar_instancia();
    std::fs::create_dir_all(pasta).map_err(|e| format!("Não consegui criar a pasta {}: {e}", pasta.display()))?;
    if origem != destino {
        let mut erro = None;
        // o .exe antigo pode demorar um pouco a ser liberado (guardião, antivírus)
        for _ in 0..50 {
            match std::fs::copy(&origem, destino) {
                Ok(_) => {
                    erro = None;
                    break;
                }
                Err(e) => {
                    erro = Some(e);
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if let Some(e) = erro {
            return Err(format!("Não consegui copiar o programa para {}: {e}", destino.display()));
        }
        // A cópia herda a marca de "baixado da internet" do instalador; sem tirar,
        // o Windows mostraria o aviso do SmartScreen de novo a cada abertura.
        let _ = std::fs::remove_file(format!("{}:Zone.Identifier", destino.display()));
    }
    for lnk in atalhos() {
        criar_atalho(&lnk, destino, "--ajustes");
    }
    registrar_desinstalador(destino, pasta);
    sistema::definir_inicio_com_windows(com_windows, destino);
    Ok(())
}

pub fn instalar(args: &[String]) {
    let silencioso = args.iter().any(|a| a == "--silencioso");
    unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED) };
    let pasta = pasta_instalacao();
    let destino = pasta.join("FlowCursor.exe");
    let anterior = versao_instalada().filter(|_| destino.exists());
    let mut com_windows = !args.iter().any(|a| a == "--sem-iniciar");

    if !silencioso {
        let (instrucao, verbo) = match &anterior {
            Some(v) if v == VERSAO => ("Reinstalar o FlowCursor".to_string(), "Reinstalar"),
            Some(v) => (format!("Atualizar o FlowCursor {v} para a versão {VERSAO}"), "Atualizar"),
            None => ("Instalar o FlowCursor".to_string(), "Instalar"),
        };
        let botao = format!("{verbo}\nCria atalhos na área de trabalho e no menu Iniciar. Não precisa de administrador.");
        let detalhes = format!(
            "Pasta: {}\n\nAtalhos de teclado: Ctrl+Alt+F9 pausa e Ctrl+Alt+F10 fecha e devolve o ponteiro normal.\n\n\
             Para desinstalar: Configurações > Aplicativos > Aplicativos instalados > FlowCursor.",
            pasta.display()
        );
        let rodape = format!("Versão {VERSAO}{}", if anterior.is_some() { " · os seus ajustes são mantidos" } else { "" });
        let (escolha, marcado) = dialogo(&Dialogo {
            instrucao: &instrucao,
            conteudo: "Troca o ponteiro do Windows por um ponteiro com mola, borrão de movimento e rastro. \
                       O clique continua no ponto real do mouse e, se algo der errado, o ponteiro normal volta sozinho.",
            botoes: &[(BOTAO_PRINCIPAL, &botao), (IDCANCEL, "Cancelar")],
            comuns: 0,
            verificacao: Some(("Abrir junto com o Windows", com_windows)),
            detalhes: Some(&detalhes),
            rodape: Some(&rodape),
            erro: false,
        });
        if escolha != BOTAO_PRINCIPAL {
            return;
        }
        com_windows = marcado;
    }

    if let Err(e) = copiar_e_registrar(&pasta, &destino, com_windows) {
        if !silencioso {
            aviso_erro("A instalação não terminou", &e);
        }
        std::process::exit(1);
    }
    if silencioso {
        // atualização pelo botão da tela de ajustes: abre a versão nova já com os ajustes
        if args.iter().any(|a| a == "--abrir") {
            let _ = Command::new(&destino).arg("--ajustes").current_dir(&pasta).spawn();
        }
        return;
    }
    dialogo(&Dialogo {
        instrucao: if anterior.is_some() { "FlowCursor atualizado" } else { "FlowCursor instalado" },
        conteudo: "O ícone fica na bandeja, perto do relógio: clique duplo nele abre os ajustes. \
                   Ctrl+Alt+F10 fecha e devolve o ponteiro normal.",
        botoes: &[(BOTAO_PRINCIPAL, "Abrir o FlowCursor\nO ponteiro novo começa a funcionar e os ajustes abrem.")],
        comuns: 0,
        verificacao: None,
        detalhes: None,
        rodape: None,
        erro: false,
    });
    let _ = Command::new(&destino).arg("--ajustes").current_dir(&pasta).spawn();
}

// ---------- Desinstalar ----------

pub fn desinstalar(args: &[String]) {
    let silencioso = args.iter().any(|a| a == "--silencioso");
    let pasta = pasta_instalacao();
    let exe = std::env::current_exe().unwrap_or_default();
    // Um .exe não consegue apagar a própria pasta enquanto roda: uma cópia
    // temporária faz o serviço.
    if exe.starts_with(&pasta) {
        let temp = std::env::temp_dir().join("FlowCursor-remover.exe");
        if std::fs::copy(&exe, &temp).is_ok() && Command::new(&temp).args(&args[1..]).current_dir(std::env::temp_dir()).spawn().is_ok() {
            return;
        }
    }
    unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED) };
    if !silencioso {
        let (escolha, _) = dialogo(&Dialogo {
            instrucao: "Desinstalar o FlowCursor",
            conteudo: "O ponteiro normal do Windows volta, e o programa, os atalhos e os ajustes são apagados.",
            botoes: &[(BOTAO_PRINCIPAL, "Desinstalar"), (IDCANCEL, "Cancelar")],
            comuns: 0,
            verificacao: None,
            detalhes: None,
            rodape: None,
            erro: false,
        });
        if escolha != BOTAO_PRINCIPAL {
            return;
        }
    }
    sistema::fechar_instancia();
    sistema::restaurar_cursores();
    sistema::definir_inicio_com_windows(false, Path::new(""));
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, w(CHAVE_DESINSTALAR).as_ptr());
    }
    for lnk in atalhos() {
        let _ = std::fs::remove_file(lnk);
    }
    // A janela de ajustes (Edge) pode demorar alguns segundos a soltar os arquivos.
    let mut apagou = false;
    for _ in 0..80 {
        if !pasta.exists() || std::fs::remove_dir_all(&pasta).is_ok() {
            apagou = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if silencioso {
        return;
    }
    if apagou {
        dialogo(&Dialogo {
            instrucao: "FlowCursor desinstalado",
            conteudo: "O ponteiro normal do Windows está de volta.",
            botoes: &[],
            comuns: TDCBF_CLOSE_BUTTON,
            verificacao: None,
            detalhes: None,
            rodape: None,
            erro: false,
        });
    } else {
        aviso_erro(
            "Quase tudo foi removido",
            &format!("Sobrou a pasta {}. Dá para apagá-la à mão depois de reiniciar o PC.", pasta.display()),
        );
    }
}
