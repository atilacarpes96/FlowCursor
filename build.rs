// Embute no .exe o ícone, o manifesto e as informações de versão (recursos/),
// compilados com o rc.exe do Windows SDK. Sem o rc.exe o programa compila
// igual, só que sem ícone próprio e com os diálogos no visual antigo.
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn achar_rc() -> Option<PathBuf> {
    let base = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versoes: Vec<PathBuf> = fs::read_dir(&base).ok()?.flatten().map(|e| e.path().join("x64").join("rc.exe")).filter(|p| p.exists()).collect();
    versoes.sort();
    versoes.pop()
}

fn main() {
    println!("cargo:rerun-if-changed=recursos");
    println!("cargo:rerun-if-changed=build.rs");
    let raiz = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let saida = PathBuf::from(env::var("OUT_DIR").unwrap());
    let versao = env::var("CARGO_PKG_VERSION").unwrap();
    let Some(rc) = achar_rc() else {
        println!("cargo:warning=rc.exe não encontrado: o .exe sai sem ícone e sem manifesto");
        return;
    };

    let manifesto = fs::read_to_string(raiz.join("recursos/flowcursor.manifest")).unwrap().replace("{VERSAO}", &format!("{versao}.0"));
    let caminho_manifesto = saida.join("flowcursor.manifest");
    fs::write(&caminho_manifesto, manifesto).unwrap();

    let numeros: Vec<&str> = versao.split('.').collect();
    let v4 = format!("{},{},{},0", numeros[0], numeros[1], numeros[2]);
    let esc = |p: PathBuf| p.display().to_string().replace('\\', "\\\\");
    let texto = format!(
        r#"1 ICON "{icone}"
1 24 "{manifesto}"
1 VERSIONINFO
FILEVERSION {v4}
PRODUCTVERSION {v4}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "041604b0"
    BEGIN
      VALUE "FileDescription", "FlowCursor"
      VALUE "FileVersion", "{versao}"
      VALUE "InternalName", "flowcursor"
      VALUE "OriginalFilename", "FlowCursor.exe"
      VALUE "ProductName", "FlowCursor"
      VALUE "ProductVersion", "{versao}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x416, 1200
  END
END
"#,
        icone = esc(raiz.join("recursos").join("flowcursor.ico")),
        manifesto = esc(caminho_manifesto),
    );
    let caminho_rc = saida.join("flowcursor.rc");
    fs::write(&caminho_rc, texto).unwrap();
    let res = saida.join("flowcursor.res");
    let ok = Command::new(rc).arg("/nologo").arg("/fo").arg(&res).arg(&caminho_rc).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    } else {
        println!("cargo:warning=rc.exe falhou: o .exe sai sem ícone e sem manifesto");
    }
}
