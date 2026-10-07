# Gera dist\FlowCursor-Instalador.exe, o arquivo para mandar para outras pessoas.
# O instalador é o próprio FlowCursor.exe: com "Instalador" no nome, ele instala.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "a compilação falhou" }
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item 'target\release\flowcursor.exe' 'dist\FlowCursor-Instalador.exe' -Force
$versao = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$tam = [math]::Round((Get-Item 'dist\FlowCursor-Instalador.exe').Length / 1KB)
Write-Host "dist\FlowCursor-Instalador.exe pronto: versão $versao, $tam KB"
