param(
  [switch]$MicrosoftStore,
  [switch]$BumpPatch,
  [string]$Version
)

$ErrorActionPreference = "Stop"

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
  $env:Path = "$cargoBin;$env:Path"
}

$tauriCli = Join-Path $PSScriptRoot "..\node_modules\.bin\tauri.cmd"
if (-not (Test-Path $tauriCli)) {
  throw "Tauri CLI not found. Run npm install first."
}

function Get-LatestNsisInstaller {
  $targetRoot = if ($env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR
  } else {
    Join-Path $PSScriptRoot "..\src-tauri\target"
  }

  $bundleDir = Resolve-Path (Join-Path $targetRoot "release\bundle\nsis") -ErrorAction SilentlyContinue
  if (-not $bundleDir) {
    throw "NSIS bundle directory was not found."
  }

  $installer = Get-ChildItem -LiteralPath $bundleDir.Path -Filter "*_x64-setup.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
  if (-not $installer) {
    throw "NSIS installer was not found."
  }

  return $installer
}

function Get-DesktopVersion {
  $configPath = Join-Path $PSScriptRoot "..\src-tauri\tauri.conf.json"
  $config = Read-Utf8Content -Path $configPath | ConvertFrom-Json
  return $config.version
}

function Assert-DesktopVersion {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Version
  )

  if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Desktop version must be production semver X.Y.Z: $Version"
  }
}

function Write-Utf8NoBom {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Path,
    [Parameter(Mandatory = $true)]
    [string] $Content
  )

  $resolved = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
  $encoding = New-Object System.Text.UTF8Encoding $false
  [System.IO.File]::WriteAllText($resolved, $Content, $encoding)
}

function Read-Utf8Content {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Path
  )

  $resolved = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
  $encoding = New-Object System.Text.UTF8Encoding $false, $true
  return [System.IO.File]::ReadAllText($resolved, $encoding)
}

function Read-Utf8Lines {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Path
  )

  $lines = @((Read-Utf8Content -Path $Path) -split "\r?\n")
  if ($lines.Count -gt 0 -and $lines[$lines.Count - 1] -eq "") {
    if ($lines.Count -eq 1) {
      return @()
    }
    return @($lines[0..($lines.Count - 2)])
  }
  return $lines
}

function Get-CargoPackageVersion {
  $cargoPath = Join-Path $PSScriptRoot "..\src-tauri\Cargo.toml"
  $inPackage = $false
  foreach ($line in Read-Utf8Lines -Path $cargoPath) {
    $trimmed = $line.Trim()
    if ($trimmed -eq "[package]") {
      $inPackage = $true
      continue
    }
    if ($inPackage -and $trimmed.StartsWith("[")) {
      break
    }
    if ($inPackage -and $trimmed -match '^version\s*=\s*"([^"]+)"\s*$') {
      return $Matches[1]
    }
  }

  throw "Cargo package version was not found."
}

function Assert-DesktopVersionFilesAligned {
  $packagePath = Join-Path $PSScriptRoot "..\package.json"
  $package = Get-Content -Raw -Path $packagePath | ConvertFrom-Json
  $versions = [ordered]@{
    "package.json" = [string]$package.version
    "src-tauri\tauri.conf.json" = [string](Get-DesktopVersion)
    "src-tauri\Cargo.toml" = [string](Get-CargoPackageVersion)
  }

  $unique = @($versions.Values | Select-Object -Unique)
  if ($unique.Count -ne 1) {
    $details = ($versions.GetEnumerator() | ForEach-Object { "$($_.Key): $($_.Value)" }) -join "; "
    throw "Desktop versions are not aligned: $details"
  }

  Assert-DesktopVersion -Version $unique[0]
}

function Get-NextPatchVersion {
  $current = Get-DesktopVersion
  Assert-DesktopVersion -Version $current
  $parts = $current.Split(".")
  return "{0}.{1}.{2}" -f $parts[0], $parts[1], ([int]$parts[2] + 1)
}

function Set-CargoPackageVersion {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Version
  )

  $cargoPath = Join-Path $PSScriptRoot "..\src-tauri\Cargo.toml"
  $lines = Read-Utf8Lines -Path $cargoPath
  $inPackage = $false
  $updated = $false
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $trimmed = $lines[$i].Trim()
    if ($trimmed -eq "[package]") {
      $inPackage = $true
      continue
    }
    if ($inPackage -and $trimmed.StartsWith("[")) {
      break
    }
    if ($inPackage -and $trimmed -match '^version\s*=') {
      $lines[$i] = 'version = "{0}"' -f $Version
      $updated = $true
      break
    }
  }

  if (-not $updated) {
    throw "Cargo package version was not found."
  }

  Write-Utf8NoBom -Path $cargoPath -Content (($lines -join [Environment]::NewLine) + [Environment]::NewLine)
}

function Set-TauriVersion {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Version
  )

  $configPath = Join-Path $PSScriptRoot "..\src-tauri\tauri.conf.json"
  $content = Read-Utf8Content -Path $configPath
  if ($content -notmatch '("version"\s*:\s*)"[^"]+"') {
    throw "Tauri config version was not found."
  }
  $updated = [regex]::Replace($content, '("version"\s*:\s*)"[^"]+"', ('$1"{0}"' -f $Version), 1)
  Write-Utf8NoBom -Path $configPath -Content $updated
}

function Set-DesktopVersion {
  param(
    [Parameter(Mandatory = $true)]
    [string] $Version
  )

  Assert-DesktopVersion -Version $Version

  Push-Location (Join-Path $PSScriptRoot "..")
  try {
    & npm version $Version --no-git-tag-version --allow-same-version
    if ($LASTEXITCODE -ne 0) {
      throw "npm version exited with code $LASTEXITCODE"
    }
  } finally {
    Pop-Location
  }

  Set-TauriVersion -Version $Version
  Set-CargoPackageVersion -Version $Version
  Assert-DesktopVersionFilesAligned
}

function Assert-PublicInstallerSignature {
  param(
    [Parameter(Mandatory = $true)]
    [System.IO.FileInfo] $Installer
  )

  $signature = Get-AuthenticodeSignature -FilePath $Installer.FullName
  if ($signature.Status -ne "Valid" -or -not $signature.SignerCertificate) {
    throw "Installer Authenticode signature is not valid: $($signature.Status) $($signature.StatusMessage)"
  }

  if ($signature.SignerCertificate.Subject -eq $signature.SignerCertificate.Issuer) {
    throw "Installer is not signed by a public trusted code signing certificate."
  }
}

function Sign-UpdaterArtifact {
  param(
    [Parameter(Mandatory = $true)]
    [System.IO.FileInfo] $Installer
  )

  if (-not $Installer.Exists) {
    Write-Warning "Updater artifact signing skipped: installer was not found."
    return
  }

  $args = @("signer", "sign")
  if ($env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    $env:TAURI_SIGNING_PRIVATE_KEY = $null
    $args += @("--private-key-path", $env:TAURI_SIGNING_PRIVATE_KEY_PATH)
  } elseif ($env:TAURI_SIGNING_PRIVATE_KEY) {
    $args += @("--private-key", $env:TAURI_SIGNING_PRIVATE_KEY)
  } else {
    Write-Warning "Updater artifact signing skipped: set TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH."
    return
  }

  $args += "--password=$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
  $args += $Installer.FullName
  & $tauriCli @args
  if ($LASTEXITCODE -ne 0) {
    throw "Tauri updater signer exited with code $LASTEXITCODE"
  }
}

function Publish-ReleaseInstaller {
  param(
    [Parameter(Mandatory = $true)]
    [System.IO.FileInfo] $Installer,
    [switch]$MicrosoftStore
  )

  $releaseDir = Join-Path $PSScriptRoot "..\release"
  New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

  if ($MicrosoftStore) {
    $version = Get-DesktopVersion
    $storeDir = Join-Path $releaseDir "microsoft-store\windows\x64\$version"
    New-Item -ItemType Directory -Force -Path $storeDir | Out-Null

    $storeName = "Setup-STEM-$version-x64.exe"
    $storePath = Join-Path $storeDir $storeName
    Copy-Item -LiteralPath $Installer.FullName -Destination $storePath -Force

    $sourceSignature = "$($Installer.FullName).sig"
    if (Test-Path $sourceSignature) {
      Copy-Item -LiteralPath $sourceSignature -Destination "$storePath.sig" -Force
    }

    Write-Host "Published Microsoft Store Windows installer: $storePath"
    Write-Host "Microsoft Store package URL: https://chat-stem.ru/downloads/microsoft-store/windows/x64/$version/$storeName"
    return
  }

  $setupPath = Join-Path $releaseDir "Setup STEM.exe"
  Copy-Item -LiteralPath $Installer.FullName -Destination $setupPath -Force

  $sourceSignature = "$($Installer.FullName).sig"
  if (Test-Path $sourceSignature) {
    Copy-Item -LiteralPath $sourceSignature -Destination (Join-Path $releaseDir "Setup STEM.exe.sig") -Force
  }

  Write-Host "Published Windows installer: $setupPath"
}

$defaultUpdaterKeyPath = Join-Path $env:USERPROFILE ".ssh\stem-tauri-updater.key"
if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and -not $env:TAURI_SIGNING_PRIVATE_KEY_PATH -and (Test-Path $defaultUpdaterKeyPath)) {
  $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $defaultUpdaterKeyPath
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
}

if ($BumpPatch -and $Version) {
  throw "Use either -BumpPatch or -Version, not both."
}

if ($BumpPatch) {
  $Version = Get-NextPatchVersion
}

if ($Version) {
  Set-DesktopVersion -Version $Version
  Write-Host "Desktop version set to $Version"
} else {
  Assert-DesktopVersionFilesAligned
}

$artifactDlib = if ($env:STEM_CODESIGN_ARTIFACT_DLIB) { $env:STEM_CODESIGN_ARTIFACT_DLIB } else { $env:STEM_CODESIGN_AZURE_DLIB }
$artifactMetadata = if ($env:STEM_CODESIGN_ARTIFACT_METADATA) { $env:STEM_CODESIGN_ARTIFACT_METADATA } else { $env:STEM_CODESIGN_AZURE_METADATA }
$hasArtifactSigningConfig = $artifactDlib -and $artifactMetadata
if (($artifactDlib -and -not $artifactMetadata) -or ($artifactMetadata -and -not $artifactDlib)) {
  throw "Artifact Signing is incomplete: set STEM_CODESIGN_ARTIFACT_DLIB and STEM_CODESIGN_ARTIFACT_METADATA."
}

$hasSigningConfig = $env:STEM_CODESIGN_THUMBPRINT -or $env:STEM_CODESIGN_PFX -or $hasArtifactSigningConfig
if (-not $hasSigningConfig) {
  if ($MicrosoftStore) {
    throw "Microsoft Store code signing is not configured. Set STEM_CODESIGN_ARTIFACT_DLIB/STEM_CODESIGN_ARTIFACT_METADATA, STEM_CODESIGN_PFX, or STEM_CODESIGN_THUMBPRINT."
  }

  Write-Warning "Public code signing is not configured. Building an unsigned installer."
}

$signConfigPath = Join-Path ([System.IO.Path]::GetTempPath()) ("stem-tauri-sign-{0}.json" -f ([System.Guid]::NewGuid().ToString("N")))
$signConfig = @{
  bundle = @{
    createUpdaterArtifacts = $false
  }
}

if ($MicrosoftStore) {
  $signConfig.bundle.windows = @{
    webviewInstallMode = @{
      type = "offlineInstaller"
    }
  }
}

if ($hasSigningConfig) {
  $signScript = Join-Path $PSScriptRoot "sign-windows.ps1"
  if (-not (Test-Path $signScript)) {
    throw "Signing script not found: $signScript"
  }

  if (-not $signConfig.bundle.windows) {
    $signConfig.bundle.windows = @{}
  }

  $signConfig.bundle.windows.signCommand = @{
    cmd = "powershell"
    args = @(
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      $signScript,
      "%1"
    )
  }
}

try {
  Write-Utf8NoBom -Path $signConfigPath -Content (($signConfig | ConvertTo-Json -Depth 8) + [Environment]::NewLine)
  & $tauriCli build --config $signConfigPath
  $buildCode = $LASTEXITCODE
  if ($buildCode -ne 0) {
    exit $buildCode
  }
  $installer = Get-LatestNsisInstaller
  if ($MicrosoftStore) {
    Assert-PublicInstallerSignature -Installer $installer
  }
  if (-not $MicrosoftStore) {
    Sign-UpdaterArtifact -Installer $installer
  }
  Publish-ReleaseInstaller -Installer $installer -MicrosoftStore:$MicrosoftStore
  exit 0
} finally {
  if (Test-Path $signConfigPath) {
    Remove-Item -LiteralPath $signConfigPath -Force
  }
}
