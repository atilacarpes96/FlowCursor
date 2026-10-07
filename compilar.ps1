# Compila o FlowCursor, troca o FlowCursor.exe desta pasta e abre a versão nova.
# Uso: powershell -ExecutionPolicy Bypass -File E:\FlowCursor\compilar.ps1 [-NaoAbrir]
param([switch]$NaoAbrir)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "a compilação falhou" }

$exe = Join-Path $PSScriptRoot 'FlowCursor.exe'
if (Test-Path $exe) {
    & $exe --sair
    # o guardião some junto; espera o arquivo ser liberado
    for ($i = 0; $i -lt 40 -and (Get-Process flowcursor -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 100 }
}
Copy-Item 'target\release\flowcursor.exe' $exe -Force
Write-Host "FlowCursor.exe atualizado"

# pelo Explorer: assim o FlowCursor não fica preso ao grupo de processos de quem rodou o script
if (-not $NaoAbrir) { Start-Process explorer.exe -ArgumentList "`"$exe`"" }
