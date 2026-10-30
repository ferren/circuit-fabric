param([Parameter(Mandatory=$true)][string]$OutPath)
$ErrorActionPreference = 'Stop'
$install = 'C:/Program Files/lceda-pro/resources/app'
$package = Get-Content -LiteralPath "$install/package.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$resourceVersions = @()
foreach ($family in @('pro-api', 'pro-ui')) {
    foreach ($directory in Get-ChildItem -LiteralPath "$install/assets/$family" -Directory) {
        $entry = @{ family = $family; version = $directory.Name; evidenceScope = 'installed resource, not active SDK handshake' }
        if ($family -eq 'pro-api') {
            $declaration = Join-Path $directory.FullName 'api.d.ts'
            if (Test-Path -LiteralPath $declaration) {
                $entry.declarationSha256 = (Get-FileHash -LiteralPath $declaration -Algorithm SHA256).Hash
            }
        }
        $resourceVersions += $entry
    }
}
$snapshot = @{
    schemaVersion = 1
    collectedAt = [DateTime]::UtcNow.ToString('o')
    packageVersion = $package.version
    processes = @(Get-Process -Name lceda-pro -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,MainWindowTitle)
    resources = $resourceVersions
    activeSdkVersion = $null
    lifecycle = 'unverified'
}
$absoluteOutput = [IO.Path]::GetFullPath($OutPath)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($absoluteOutput)) | Out-Null
[IO.File]::WriteAllText($absoluteOutput, ($snapshot | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
Write-Output $OutPath
