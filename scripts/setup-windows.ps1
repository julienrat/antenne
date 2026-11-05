# Prépare un poste Windows 10/11 pour développer Antenne.
# Lancer dans PowerShell :  powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
$ErrorActionPreference = "Stop"

Write-Host "==> Rust (rustup)"
winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements

Write-Host "==> Outils de compilation MSVC (C++ build tools)"
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

Write-Host "==> CMake (compilation de libopus pour la feature opus)"
winget install --id Kitware.CMake -e

Write-Host "==> ffmpeg (ffplay pour écouter les flux de test)"
winget install --id Gyan.FFmpeg -e

Write-Host ""
Write-Host "==> Pilote de carte son virtuelle (manuel)"
Write-Host "    VB-CABLE (gratuit, 1 câble stéréo) : https://vb-audio.com/Cable/"
Write-Host "    Plusieurs cartes : Virtual Audio Cable (payant) ou VB-CABLE A+B / C+D."
Write-Host "    Puis renseigner platform.loopback_device dans config/antenne.toml"
Write-Host "    (nom visible avec : cargo run -p antenne -- devices)."
Write-Host ""
Write-Host "==> Terminé. Rouvrir le terminal puis : cargo build ; cargo test"
