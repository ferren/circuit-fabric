param(
    [string]$OutputPath = "artifacts/circuitfabric-jlc-eda-extension_v0.2.2.eext"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$extensionRoot = Join-Path $root "plugins/jlcircuit-eda-extension"
$output = Join-Path $root $OutputPath
$temporaryZip = "$output.zip"
$outputParent = Split-Path -Parent $output

New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
if (Test-Path -LiteralPath $output) {
    Remove-Item -LiteralPath $output -Force
}
if (Test-Path -LiteralPath $temporaryZip) {
    Remove-Item -LiteralPath $temporaryZip -Force
}
Compress-Archive -Path @(
    (Join-Path $extensionRoot "extension.json"),
    (Join-Path $extensionRoot "dist"),
    (Join-Path $extensionRoot "iframe"),
    (Join-Path $extensionRoot "assets")
) -DestinationPath $temporaryZip
Move-Item -LiteralPath $temporaryZip -Destination $output
Write-Output $output
