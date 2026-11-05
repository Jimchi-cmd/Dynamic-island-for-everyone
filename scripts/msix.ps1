# Build and sign Islet's MSIX package (Windows, needs the Windows SDK).
#
#   scripts/msix.ps1 -Version 0.1.0 -Out out [-Bin target/release]
#
# Signing: with MSIX_PFX_BASE64 (and MSIX_PFX_PASSWORD) in the environment
# it signs with that certificate, and the package's Publisher is the
# certificate's subject. Without them it makes a throwaway self-signed
# test certificate ("CN=Islet Test") and writes its public part next to
# the package as Islet-test-certificate.cer, so a tester can trust it.
# Nothing secret is written to the output folder.
param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$Out = "out",
  [string]$Bin = "target/release"
)
$ErrorActionPreference = "Stop"

$kits = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
  Sort-Object FullName -Descending | Select-Object -First 1
if (-not $kits) { throw "makeappx.exe not found: install the Windows 10/11 SDK" }
$sdk = $kits.DirectoryName
$makeappx = Join-Path $sdk "makeappx.exe"
$signtool = Join-Path $sdk "signtool.exe"

$work = Join-Path ([IO.Path]::GetTempPath()) ("islet-msix-" + [Guid]::NewGuid())
$stage = Join-Path $work "stage"
New-Item -ItemType Directory -Force -Path $stage, $Out | Out-Null
try {
  # Certificate first: the manifest's Publisher must match its subject.
  $pfx = Join-Path $work "sign.pfx"
  if ($env:MSIX_PFX_BASE64) {
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:MSIX_PFX_BASE64))
    $password = $env:MSIX_PFX_PASSWORD
    $cert = New-Object Security.Cryptography.X509Certificates.X509Certificate2($pfx, $password)
    $publisher = $cert.Subject
  } else {
    $password = [Guid]::NewGuid().ToString()
    $publisher = "CN=Islet Test"
    $cert = New-SelfSignedCertificate -Type Custom -Subject $publisher -KeyUsage DigitalSignature `
      -FriendlyName "Islet test package signing" -CertStoreLocation "Cert:\CurrentUser\My" `
      -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
    Export-PfxCertificate -Cert $cert -FilePath $pfx -Password (ConvertTo-SecureString $password -AsPlainText -Force) | Out-Null
    Export-Certificate -Cert $cert -FilePath (Join-Path $Out "Islet-test-certificate.cer") | Out-Null
    Remove-Item "Cert:\CurrentUser\My\$($cert.Thumbprint)"
  }

  # MSIX versions have four numbers: 0.1.0 -> 0.1.0.0.
  $parts = ($Version -split '[-+]')[0].Split('.')
  while ($parts.Count -lt 4) { $parts += "0" }
  $ver = ($parts[0..3] -join '.')

  $manifest = Get-Content "packaging/msix/AppxManifest.xml" -Raw
  $manifest = $manifest.Replace("@VERSION@", $ver).Replace("@PUBLISHER@", [Security.SecurityElement]::Escape($publisher))
  Set-Content -Path (Join-Path $stage "AppxManifest.xml") -Value $manifest -Encoding UTF8
  Copy-Item -Recurse "packaging/msix/Assets" (Join-Path $stage "Assets")
  Copy-Item (Join-Path $Bin "islet.exe"), (Join-Path $Bin "islet-settings.exe") $stage
  Copy-Item "LICENSE" (Join-Path $stage "LICENSE.txt")

  $msix = Join-Path $Out "Islet_${Version}_x64.msix"
  & $makeappx pack /o /d $stage /p $msix
  if ($LASTEXITCODE -ne 0) { throw "makeappx failed" }
  & $signtool sign /fd SHA256 /f $pfx /p $password $msix
  if ($LASTEXITCODE -ne 0) { throw "signtool failed" }
  Write-Host "Built $msix (publisher $publisher)"
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
