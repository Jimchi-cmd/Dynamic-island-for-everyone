<#
Make Islet's release keys on YOUR computer and hand them to GitHub (Windows).

  powershell -ExecutionPolicy Bypass -File setup-signing.ps1
  powershell -ExecutionPolicy Bypass -File setup-signing.ps1 -NoUpload

It makes two keys, in %USERPROFILE%\islet-keys (never inside the repository):
  - the phone app's signing key, so each new APK installs over the last one;
  - the update key, so Islet on a computer can update itself in place.
With the GitHub command line (`gh`, signed in) it adds them to the
repository's secrets; otherwise it writes a file saying what to paste where.
Back the folder up (a password manager or an encrypted drive): if a key is
lost, phones must uninstall and reinstall, and computers must download the
next version by hand.

Needs keytool (Java 17+, or Android Studio) and Node.js. Missing ones:
  winget install Microsoft.OpenJDK.21
  winget install OpenJS.NodeJS.LTS
#>
param([switch]$NoUpload)
$ErrorActionPreference = 'Stop'

$dir = if ($env:ISLET_KEYS_DIR) { $env:ISLET_KEYS_DIR } else { Join-Path $env:USERPROFILE 'islet-keys' }
$repo = if ($env:ISLET_REPO) { $env:ISLET_REPO } else { 'Jimchi-cmd/Dynamic-island-for-everyone' }

function Find-Keytool {
    $c = Get-Command keytool -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    $places = @()
    if ($env:JAVA_HOME) { $places += Join-Path $env:JAVA_HOME 'bin\keytool.exe' }
    $places += Join-Path $env:ProgramFiles 'Android\Android Studio\jbr\bin\keytool.exe'
    # Microsoft's OpenJDK (winget) and Adoptium's Temurin.
    $places += Get-ChildItem -Path (Join-Path $env:ProgramFiles 'Microsoft\jdk-*\bin\keytool.exe') -ErrorAction SilentlyContinue | ForEach-Object FullName
    $places += Get-ChildItem -Path (Join-Path $env:ProgramFiles 'Eclipse Adoptium\*\bin\keytool.exe') -ErrorAction SilentlyContinue | ForEach-Object FullName
    foreach ($p in $places) { if ($p -and (Test-Path $p)) { return $p } }
    throw 'keytool not found: install Java with "winget install Microsoft.OpenJDK.21" (or Android Studio), then run this again.'
}

# 32 letters and digits from the system's secure random generator.
function New-Password {
    $chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789'
    $bytes = New-Object byte[] 32
    [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    -join ($bytes | ForEach-Object { $chars[$_ % $chars.Length] })
}

function Read-Saved([string]$name) {
    $f = Join-Path $dir 'passwords.txt'
    if (-not (Test-Path $f)) { return $null }
    $line = Get-Content $f | Where-Object { $_ -like "$name=*" } | Select-Object -First 1
    if ($line) { return $line.Substring($name.Length + 1) }
    return $null
}

$keytool = Find-Keytool
if (-not (Get-Command npx.cmd -ErrorAction SilentlyContinue)) {
    throw 'npx not found: install Node.js with "winget install OpenJS.NodeJS.LTS", then open a new PowerShell and run this again.'
}
New-Item -ItemType Directory -Force -Path $dir | Out-Null

# 1. The phone app's signing key.
$jks = Join-Path $dir 'islet-release.jks'
if (Test-Path $jks) {
    Write-Host "Keeping the phone key already in $jks"
    $storePw = Read-Saved 'ANDROID_KEYSTORE_PASSWORD'
} else {
    $storePw = New-Password
    & $keytool -genkeypair -keystore $jks -storetype PKCS12 -alias islet -keyalg RSA -keysize 4096 `
        -validity 10000 -dname 'CN=Islet' -storepass $storePw -keypass $storePw | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'keytool could not make the phone key.' }
    Write-Host "Made the phone key: $jks"
}

# 2. The update key (Tauri's, Ed25519).
$key = Join-Path $dir 'islet-updater.key'
if (Test-Path $key) {
    Write-Host "Keeping the update key already in $key"
    $updPw = Read-Saved 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD'
} else {
    $updPw = New-Password
    $cli = '2'
    $pkg = Join-Path $PSScriptRoot '..\package.json'
    if (Test-Path $pkg) {
        $v = (Get-Content $pkg -Raw | ConvertFrom-Json).devDependencies.'@tauri-apps/cli'
        if ($v) { $cli = $v -replace '^[^0-9]*', '' }
    }
    & npx.cmd --yes "@tauri-apps/cli@$cli" signer generate --ci -w $key -p $updPw | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Tauri could not make the update key.' }
    Write-Host "Made the update key: $key"
}

Set-Content -Path (Join-Path $dir 'passwords.txt') -Value @(
    "ANDROID_KEYSTORE_PASSWORD=$storePw",
    'ANDROID_KEY_ALIAS=islet',
    "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$updPw"
)

$jksB64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($jks))
$priv = (Get-Content $key -Raw).Trim()
$pub = (Get-Content "$key.pub" -Raw).Trim()
$secrets = [ordered]@{
    ANDROID_KEYSTORE_BASE64            = $jksB64
    ANDROID_KEYSTORE_PASSWORD          = $storePw
    ANDROID_KEY_ALIAS                  = 'islet'
    ANDROID_KEY_PASSWORD               = $storePw
    TAURI_SIGNING_PRIVATE_KEY          = $priv
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $updPw
}

$gh = Get-Command gh -ErrorAction SilentlyContinue
$signedIn = $false
if ($gh -and -not $NoUpload) {
    & gh auth status *> $null
    $signedIn = ($LASTEXITCODE -eq 0)
}
if ($signedIn) {
    Write-Host "Adding them to $repo (Settings > Secrets and variables > Actions)..."
    foreach ($name in $secrets.Keys) {
        & gh secret set $name --repo $repo --body $secrets[$name]
        if ($LASTEXITCODE -ne 0) { throw "gh could not set $name." }
    }
    & gh variable set ISLET_UPDATER_PUBKEY --repo $repo --body $pub
    if ($LASTEXITCODE -ne 0) { throw 'gh could not set ISLET_UPDATER_PUBKEY.' }
    Write-Host 'Done. The next release is signed with these keys.'
} else {
    $out = Join-Path $dir 'paste-into-github.txt'
    $lines = @(
        "Paste each of these at https://github.com/$repo/settings/secrets/actions",
        '(Secrets: "New repository secret"; the last one under the Variables tab.)',
        ''
    )
    foreach ($name in $secrets.Keys) { $lines += "Secret ${name}:", $secrets[$name], '' }
    $lines += 'Variable ISLET_UPDATER_PUBKEY:', $pub
    Set-Content -Path $out -Value $lines
    Write-Host "Wrote what to paste into GitHub: $out (delete it once pasted)."
}
Write-Host ''
Write-Host "Back up $dir now: without it, phones must reinstall and computers must update by hand."
