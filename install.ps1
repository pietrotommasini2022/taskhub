$ErrorActionPreference = "Stop"

$Repo = "pietroairoldi/taskhub"
$InstallDir = "$env:USERPROFILE\.local\bin"

$Release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name
$Asset = $Release.assets | Where-Object { $_.name -like "*windows*" } | Select-Object -First 1

if (-not $Asset) {
    Write-Error "No Windows release found for $Tag"
    exit 1
}

Write-Host "Downloading TaskHub $Tag..."
$Tmp = [System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid().ToString()
New-Item -ItemType Directory -Path $Tmp | Out-Null

$ZipPath = "$Tmp\taskhub.zip"
Invoke-WebRequest $Asset.browser_download_url -OutFile $ZipPath
Expand-Archive $ZipPath -DestinationPath $Tmp

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item "$Tmp\taskhub.exe" "$InstallDir\taskhub.exe" -Force
Remove-Item $Tmp -Recurse -Force

# Add to user PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to your PATH (restart terminal to take effect)"
}

Write-Host "Installed: $InstallDir\taskhub.exe"
Write-Host "Run 'taskhub init' to get started."
