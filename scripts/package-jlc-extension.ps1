param(
    [string]$OutputPath = "artifacts/circuitfabric-jlc-eda-extension_v0.2.10.eext"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$extensionRoot = Join-Path $root "plugins/jlcircuit-eda-extension"
$entrySource = Join-Path $extensionRoot "src/index.js"
if (!(Test-Path -LiteralPath $entrySource)) { throw "Missing extension entry source: $entrySource" }
New-Item -ItemType Directory -Force -Path (Join-Path $extensionRoot "dist") | Out-Null
Copy-Item -LiteralPath $entrySource -Destination (Join-Path $extensionRoot "dist/index.js")
$output = Join-Path $root $OutputPath
$temporaryZip = "$output.tmp"
$outputParent = Split-Path -Parent $output

New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
if (Test-Path -LiteralPath $output) {
    Remove-Item -LiteralPath $output -Force
}
if (Test-Path -LiteralPath $temporaryZip) {
    Remove-Item -LiteralPath $temporaryZip -Force
}
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$files = @(
    @{ Source = (Join-Path $extensionRoot "extension.json"); Entry = "extension.json" },
    @{ Source = (Join-Path $extensionRoot "dist/index.js"); Entry = "dist/index.js" },
    @{ Source = (Join-Path $extensionRoot "iframe/index.html"); Entry = "iframe/index.html" },
    @{ Source = (Join-Path $extensionRoot "iframe/compat.js"); Entry = "iframe/compat.js" },
    @{ Source = (Join-Path $extensionRoot "assets/circuitfabric-logo.png"); Entry = "assets/circuitfabric-logo.png" }
)
$archive = [System.IO.Compression.ZipFile]::Open($temporaryZip, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($file in $files) {
        $entry = $archive.CreateEntry($file.Entry, [System.IO.Compression.CompressionLevel]::Optimal)
        $input = [System.IO.File]::OpenRead($file.Source)
        $destination = $entry.Open()
        try { $input.CopyTo($destination) } finally { $destination.Dispose(); $input.Dispose() }
    }
} finally { $archive.Dispose() }
Move-Item -LiteralPath $temporaryZip -Destination $output
Write-Output $output
