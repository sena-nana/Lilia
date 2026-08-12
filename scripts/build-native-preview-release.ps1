param(
  [Parameter(Mandatory = $true)]
  [string]$Tag,
  [string]$OutputRoot = "artifacts/native-preview"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$tagPrefix = "native-preview-v"
if (-not $Tag.StartsWith($tagPrefix, [StringComparison]::Ordinal)) {
  throw "Native Preview tag must start with $tagPrefix."
}
$version = $Tag.Substring($tagPrefix.Length)
if ($version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
  throw "Native Preview tag contains an invalid semantic version: $version"
}

$cargoManifest = Get-Content -Raw (Join-Path $repoRoot "Cargo.toml")
$escapedVersion = [Regex]::Escape($version)
if ($cargoManifest -notmatch ('(?m)^version\s*=\s*"{0}"\s*$' -f $escapedVersion)) {
  throw "Native Preview tag version $version does not match workspace.package.version."
}
if (-not $env:LILIA_NATIVE_UPDATER_PUBKEY) {
  throw "LILIA_NATIVE_UPDATER_PUBKEY is required for a release build."
}
if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and -not $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
  throw "TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH is required to sign the updater archive."
}
if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
  throw "TAURI_SIGNING_PRIVATE_KEY_PASSWORD is required to sign the updater archive."
}

$makensisCommand = Get-Command makensis.exe -ErrorAction SilentlyContinue
$makensisPath = if ($makensisCommand) { $makensisCommand.Source } else { $null }
if (-not $makensisPath) {
  $knownMakensis = @(
    (Join-Path ${env:ProgramFiles(x86)} "NSIS\makensis.exe"),
    (Join-Path $env:LOCALAPPDATA "tauri\NSIS\makensis.exe")
  ) | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($knownMakensis) {
    $makensisPath = $knownMakensis
  } else {
    throw "makensis.exe is required. Install NSIS before building the Native Preview installer."
  }
}

$outputBase = if ([IO.Path]::IsPathRooted($OutputRoot)) {
  $OutputRoot
} else {
  Join-Path $repoRoot $OutputRoot
}
$outputDir = Join-Path $outputBase $version
if (Test-Path $outputDir) {
  throw "Native Preview release output already exists: $outputDir"
}
New-Item -ItemType Directory -Path $outputDir | Out-Null

Push-Location $repoRoot
try {
  & cargo build --locked --release -p lilia-native-preview
  if ($LASTEXITCODE -ne 0) {
    throw "Native Preview release build failed."
  }

  $binary = Join-Path $repoRoot "target\release\lilia-native-preview.exe"
  $hostLibrary = Join-Path $repoRoot "target\release\lilia_native_host.dll"
  if (-not (Test-Path $binary)) {
    throw "Native Preview release binary was not produced: $binary"
  }
  if (-not (Test-Path $hostLibrary)) {
    throw "Native Preview release host library was not produced: $hostLibrary"
  }
  & node (Join-Path $repoRoot "scripts\verify-native-agent-debug-release-exclusion.mjs") --binary $binary --binary $hostLibrary
  if ($LASTEXITCODE -ne 0) {
    throw "Native Preview release binary contains development-only Agent Debug markers."
  }
  $installerName = "LiliaCodeNativePreview_${version}_x64-setup.exe"
  $installer = Join-Path $outputDir $installerName
  $icon = Join-Path $repoRoot "apps\desktop\src-tauri\icons\icon.ico"
  $script = Join-Path $repoRoot "apps\native-desktop\windows\installer.nsi"
  & $makensisPath "/INPUTCHARSET" "UTF8" "/DAPP_VERSION=$version" "/DNATIVE_BINARY=$binary" "/DNATIVE_HOST_LIBRARY=$hostLibrary" "/DOUTPUT_FILE=$installer" "/DAPP_ICON=$icon" $script
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $installer)) {
    throw "Native Preview NSIS installer build failed."
  }

  $archiveName = "$installerName.nsis.zip"
  $archive = Join-Path $outputDir $archiveName
  Compress-Archive -LiteralPath $installer -DestinationPath $archive -CompressionLevel Optimal
  & corepack yarn tauri signer sign $archive
  if ($LASTEXITCODE -ne 0) {
    throw "Native Preview updater signing failed."
  }
  $signaturePath = "$archive.sig"
  if (-not (Test-Path $signaturePath)) {
    throw "Native Preview updater signature was not produced: $signaturePath"
  }
  $signature = (Get-Content -Raw $signaturePath).Trim()
  if (-not $signature) {
    throw "Native Preview updater signature is empty."
  }

  $repository = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { "sena-nana/LiliaCode" }
  $downloadUrl = "https://github.com/$repository/releases/download/$Tag/$archiveName"
  $manifest = [ordered]@{
    version = $version
    notes = "LiliaCode Native Preview $version"
    pub_date = [DateTimeOffset]::UtcNow.ToString("o")
    platforms = [ordered]@{
      "windows-x86_64-nsis" = [ordered]@{
        url = $downloadUrl
        signature = $signature
      }
    }
  }
  $manifestPath = Join-Path $outputDir "latest-native-preview.json"
  $manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

  [ordered]@{
    tag = $Tag
    version = $version
    outputDir = $outputDir
    installer = $installer
    archive = $archive
    signature = $signaturePath
    manifest = $manifestPath
  } | ConvertTo-Json -Depth 3
} finally {
  Pop-Location
}
