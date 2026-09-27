param(
    [string]$OutputPath = "native/windows-x64/typesafe-mcp"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root "vendor/typesafe-mcp"
if (!(Test-Path -LiteralPath (Join-Path $source "go.mod"))) {
    throw "Missing vendored source: $source (run `git submodule update --init`)"
}
if (!(Get-Command go -ErrorAction SilentlyContinue)) { throw "Go toolchain not found on PATH" }

$version = git -C $source describe --tags 2>$null
if (!$version) { $version = "dev" }

$output = Join-Path $root $OutputPath
New-Item -ItemType Directory -Force -Path $output | Out-Null
$binary = Join-Path $output "evaluate.exe"

Push-Location $source
try {
    go build -trimpath -ldflags "-s -w -X main.version=$version" -o $binary ./cmd/evaluate
    if ($LASTEXITCODE -ne 0) { throw "go build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
Copy-Item -LiteralPath (Join-Path $source "LICENSE") -Destination (Join-Path $output "LICENSE")
Set-Content -LiteralPath (Join-Path $output "VERSION") -Value $version -NoNewline
Write-Output $binary
