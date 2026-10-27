param(
    [string]$OutPath = "shot.png",
    [int]$Left = -1,
    [int]$Top = -1,
    [int]$Width = -1,
    [int]$Height = -1
)
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

if ($Left -lt 0) {
    # capture virtual screen (all monitors)
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $Left = $bounds.Left; $Top = $bounds.Top; $Width = $bounds.Width; $Height = $bounds.Height
}
$bmp = New-Object System.Drawing.Bitmap $Width, $Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($Left, $Top, 0, 0, $bmp.Size)
$bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output $OutPath
