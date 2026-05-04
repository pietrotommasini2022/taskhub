$ErrorActionPreference = "Stop"

$repoRoot = Split-Path $PSScriptRoot -Parent
$pluginsSrc = Join-Path $repoRoot "plugins"
$installDir = Join-Path $env:USERPROFILE ".taskhub\plugins"

$plugins = @("weather", "github", "gitlab", "slack", "discord", "rss", "linear", "notion", "calendar_caldav", "s3", "llm")

foreach ($plugin in $plugins) {
    $src = Join-Path $pluginsSrc $plugin
    if (-not (Test-Path (Join-Path $src "Cargo.toml"))) {
        Write-Host "SKIP $plugin (no Cargo.toml)"
        continue
    }

    Write-Host "Building $plugin..."
    Push-Location $src
    cargo build --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  ERROR: build failed for $plugin"
        Pop-Location
        continue
    }
    Pop-Location

    $wasmFile = Get-ChildItem (Join-Path $src "target\wasm32-unknown-unknown\release") -Filter "*.wasm" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $wasmFile) {
        Write-Host "  ERROR: no .wasm output for $plugin"
        continue
    }

    $pluginToml = Join-Path $src "plugin.toml"
    $pluginId = (Get-Content $pluginToml | Where-Object { $_ -match '^id\s*=' } | Select-Object -First 1) -replace 'id\s*=\s*"(.+)"', '$1'

    $dest = Join-Path $installDir $pluginId
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item $pluginToml (Join-Path $dest "plugin.toml") -Force
    Copy-Item $wasmFile.FullName (Join-Path $dest "plugin.wasm") -Force
    Write-Host "  Installed $pluginId -> $dest"
}

Write-Host ""
Write-Host "Done. Run 'taskhub plugin list' to verify."
