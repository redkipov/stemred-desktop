param(
  [Parameter(Mandatory = $true)]
  [string]$File
)

$ErrorActionPreference = "Stop"

function Resolve-SignTool {
  if ($env:STEM_SIGNTOOL_PATH) {
    if (Test-Path $env:STEM_SIGNTOOL_PATH) {
      return $env:STEM_SIGNTOOL_PATH
    }

    throw "STEM_SIGNTOOL_PATH is set, but the file was not found: $env:STEM_SIGNTOOL_PATH"
  }

  $fromPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($fromPath) {
    return $fromPath.Source
  }

  $kitsRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
  $fromSdk = Get-ChildItem $kitsRoot -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1

  if ($fromSdk) {
    return $fromSdk.FullName
  }

  throw "signtool.exe was not found. Install Windows SDK or set STEM_SIGNTOOL_PATH."
}

if (-not (Test-Path $File)) {
  throw "File to sign was not found: $File"
}

$signtool = Resolve-SignTool
$artifactDlib = if ($env:STEM_CODESIGN_ARTIFACT_DLIB) { $env:STEM_CODESIGN_ARTIFACT_DLIB } else { $env:STEM_CODESIGN_AZURE_DLIB }
$artifactMetadata = if ($env:STEM_CODESIGN_ARTIFACT_METADATA) { $env:STEM_CODESIGN_ARTIFACT_METADATA } else { $env:STEM_CODESIGN_AZURE_METADATA }
$usesArtifactSigning = $artifactDlib -or $artifactMetadata
$timestampUrl = if ($env:STEM_CODESIGN_TIMESTAMP_URL) {
  $env:STEM_CODESIGN_TIMESTAMP_URL
} elseif ($usesArtifactSigning) {
  "http://timestamp.acs.microsoft.com"
} else {
  "http://timestamp.digicert.com"
}

$args = @(
  "sign",
  "/fd", "SHA256",
  "/td", "SHA256",
  "/tr", $timestampUrl,
  "/d", "StemRed",
  "/du", "https://chat-stem.ru/"
)

if ($usesArtifactSigning) {
  if (-not $artifactDlib -or -not $artifactMetadata) {
    throw "Artifact Signing is incomplete: set STEM_CODESIGN_ARTIFACT_DLIB and STEM_CODESIGN_ARTIFACT_METADATA."
  }

  if (-not (Test-Path $artifactDlib)) {
    throw "Artifact Signing DLIB was not found: $artifactDlib"
  }

  if (-not (Test-Path $artifactMetadata)) {
    throw "Artifact Signing metadata file was not found: $artifactMetadata"
  }

  $args += @("/dlib", $artifactDlib, "/dmdf", $artifactMetadata)
} elseif ($env:STEM_CODESIGN_PFX) {
  if (-not (Test-Path $env:STEM_CODESIGN_PFX)) {
    throw "PFX file was not found: $env:STEM_CODESIGN_PFX"
  }

  $args += @("/f", $env:STEM_CODESIGN_PFX)
  if ($env:STEM_CODESIGN_PFX_PASSWORD) {
    $args += @("/p", $env:STEM_CODESIGN_PFX_PASSWORD)
  }
} elseif ($env:STEM_CODESIGN_THUMBPRINT) {
  if ($env:STEM_CODESIGN_STORE_LOCATION -eq "LocalMachine") {
    $args += "/sm"
  }

  $args += @("/sha1", $env:STEM_CODESIGN_THUMBPRINT)
} else {
  throw "Signing certificate is not configured: use STEM_CODESIGN_ARTIFACT_DLIB/STEM_CODESIGN_ARTIFACT_METADATA, STEM_CODESIGN_THUMBPRINT, or STEM_CODESIGN_PFX."
}

$args += @("/v", $File)

& $signtool @args
if ($LASTEXITCODE -ne 0) {
  throw "signtool exited with code $LASTEXITCODE"
}
