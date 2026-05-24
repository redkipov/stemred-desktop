param(
  [switch]$MicrosoftStore
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
  $bundleDir = Resolve-Path (Join-Path $PSScriptRoot "..\src-tauri\target\release\bundle\nsis") -ErrorAction SilentlyContinue
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
  $config = Get-Content -Raw -Path $configPath | ConvertFrom-Json
  return $config.version
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
    throw "Installer is signed by a self-signed certificate. Microsoft Store requires a public trusted code signing certificate."
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

$artifactDlib = if ($env:STEM_CODESIGN_ARTIFACT_DLIB) { $env:STEM_CODESIGN_ARTIFACT_DLIB } else { $env:STEM_CODESIGN_AZURE_DLIB }
$artifactMetadata = if ($env:STEM_CODESIGN_ARTIFACT_METADATA) { $env:STEM_CODESIGN_ARTIFACT_METADATA } else { $env:STEM_CODESIGN_AZURE_METADATA }
$hasArtifactSigningConfig = $artifactDlib -and $artifactMetadata
if (($artifactDlib -and -not $artifactMetadata) -or ($artifactMetadata -and -not $artifactDlib)) {
  throw "Artifact Signing is incomplete: set STEM_CODESIGN_ARTIFACT_DLIB and STEM_CODESIGN_ARTIFACT_METADATA."
}

$allowLocalCodeSigning = $env:STEM_ALLOW_LOCAL_CODESIGN -eq "1"
if ($MicrosoftStore -and $allowLocalCodeSigning) {
  throw "Microsoft Store build cannot use local self-signed code signing. Configure public code signing first."
}

if (-not $env:STEM_CODESIGN_THUMBPRINT -and -not $env:STEM_CODESIGN_PFX -and -not $hasArtifactSigningConfig -and $allowLocalCodeSigning) {
  $defaultCodeSigningThumbprint = "6794A2C99DE2F5363E56F8384F905159E2E903AE"
  $localCodeSigningCert = Get-Item "Cert:\CurrentUser\My\$defaultCodeSigningThumbprint" -ErrorAction SilentlyContinue
  if (-not $localCodeSigningCert) {
    $localCodeSigningCert = Get-ChildItem Cert:\CurrentUser\My -ErrorAction SilentlyContinue |
      Where-Object { $_.Subject -eq "CN=STEM Messenger Local Code Signing" -and $_.HasPrivateKey } |
      Sort-Object NotAfter -Descending |
      Select-Object -First 1
  }

  if ($localCodeSigningCert) {
    $env:STEM_CODESIGN_THUMBPRINT = $localCodeSigningCert.Thumbprint
  } else {
    $env:STEM_CODESIGN_THUMBPRINT = $defaultCodeSigningThumbprint
  }
}

$hasSigningConfig = $env:STEM_CODESIGN_THUMBPRINT -or $env:STEM_CODESIGN_PFX -or $hasArtifactSigningConfig
if (-not $hasSigningConfig) {
  if ($MicrosoftStore) {
    throw "Microsoft Store code signing is not configured. Set STEM_CODESIGN_ARTIFACT_DLIB/STEM_CODESIGN_ARTIFACT_METADATA, STEM_CODESIGN_PFX, or STEM_CODESIGN_THUMBPRINT. Self-signed certificates are not accepted."
  }

  throw "Public code signing is not configured. Set STEM_CODESIGN_ARTIFACT_DLIB/STEM_CODESIGN_ARTIFACT_METADATA, STEM_CODESIGN_PFX, or STEM_CODESIGN_THUMBPRINT. For local testing only, set STEM_ALLOW_LOCAL_CODESIGN=1."
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
  $signConfig | ConvertTo-Json -Depth 8 | Set-Content -Path $signConfigPath -Encoding UTF8
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
